//! The `[mail]` section: the public client id, where accounts live and the
//! attachment ceiling. The permission list is not configurable here: it is
//! `SCOPES`, checked by [`assert_scopes_safe`] on every configuration load.

use std::path::{Path, PathBuf};

use excubia_persist::migrate::{adopt_dir, chosen_dir};
use serde::Deserialize;

use crate::auth::assert_scopes_safe;
use crate::error::MailError;
use crate::redact::Redacted;

/// Environment variable that supplies `client_id` when the file does not.
pub const CLIENT_ID_ENV: &str = "EXCUBIA_MAIL_CLIENT_ID";

/// Attachment ceiling by default (MAIL1B policy, shared with the local path
/// policy): 25 MiB.
pub const DEFAULT_MAX_ATTACHMENT_BYTES: u64 = 25 * 1024 * 1024;

/// The policy never accepts more than this, whatever the file says.
pub const HARD_MAX_ATTACHMENT_BYTES: u64 = 150 * 1024 * 1024;

/// Mails one account may send per hour, unless `mail.toml` says otherwise (MAIL2A).
pub const DEFAULT_SENDS_PER_HOUR: u32 = 10;

/// The ceiling `mail.toml` may set for `max_sends_per_hour`.
pub const HARD_MAX_SENDS_PER_HOUR: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailConfig {
    pub client_id: String,
    pub accounts_dir: PathBuf,
    pub max_attachment_bytes: u64,
    pub max_sends_per_hour: u32,
}

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    mail: Option<RawMail>,
}

#[derive(Debug, Default, Deserialize)]
struct RawMail {
    client_id: Option<String>,
    accounts_dir: Option<PathBuf>,
    max_attachment_bytes: Option<u64>,
    max_sends_per_hour: Option<u32>,
}

impl MailConfig {
    /// Read `path` and resolve `client_id` from the file or the environment.
    pub fn load(path: &Path) -> Result<Self, MailError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|err| MailError::InvalidConfig(Redacted::new(err.to_string())))?;
        let env_client_id = std::env::var(CLIENT_ID_ENV).ok();
        Self::from_toml(&raw, env_client_id.as_deref())
    }

    /// The pure half of [`MailConfig::load`], for callers and tests that
    /// already have the text and the environment value.
    pub fn from_toml(raw: &str, env_client_id: Option<&str>) -> Result<Self, MailError> {
        let parsed: RawConfig = toml::from_str(raw)
            .map_err(|err| MailError::InvalidConfig(Redacted::new(err.to_string())))?;
        let mail = parsed.mail.unwrap_or_default();

        let client_id = mail
            .client_id
            .or_else(|| env_client_id.map(str::to_owned))
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .ok_or(MailError::NotConfigured)?;

        let max_attachment_bytes = mail
            .max_attachment_bytes
            .unwrap_or(DEFAULT_MAX_ATTACHMENT_BYTES);
        if max_attachment_bytes == 0 || max_attachment_bytes > HARD_MAX_ATTACHMENT_BYTES {
            return Err(MailError::InvalidConfig(Redacted::new(format!(
                "max_attachment_bytes must be between 1 and {HARD_MAX_ATTACHMENT_BYTES}"
            ))));
        }

        let max_sends_per_hour = mail.max_sends_per_hour.unwrap_or(DEFAULT_SENDS_PER_HOUR);
        if max_sends_per_hour == 0 || max_sends_per_hour > HARD_MAX_SENDS_PER_HOUR {
            return Err(MailError::InvalidConfig(Redacted::new(format!(
                "max_sends_per_hour must be between 1 and {HARD_MAX_SENDS_PER_HOUR}"
            ))));
        }

        assert_scopes_safe()?;
        Ok(Self {
            client_id,
            accounts_dir: mail.accounts_dir.unwrap_or_else(default_accounts_dir),
            max_attachment_bytes,
            max_sends_per_hour,
        })
    }
}

/// `EXCUBIA_DATA_DIR`, then the platform data directory, then a local folder:
/// a host that does not configure the path still gets a private directory.
/// Until [`migrate_default_dir`] moves it, the old folder is read.
fn default_accounts_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("EXCUBIA_DATA_DIR") {
        return PathBuf::from(dir).join("mail").join("accounts");
    }
    // legacy: `.herdr-notify` is the pre-rename fallback, used only without a platform folder.
    let (new, old) = mail_dirs().unwrap_or_else(|| {
        (
            PathBuf::from(".excubia").join("mail"),
            PathBuf::from(".herdr-notify").join("mail"),
        )
    });
    chosen_dir(&new, &old).join("accounts")
}

/// The mail folder under its new name and under the old one, on this platform.
fn mail_dirs() -> Option<(PathBuf, PathBuf)> {
    #[cfg(windows)]
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        return Some(mail_dirs_under(PathBuf::from(dir)));
    }
    #[cfg(not(windows))]
    {
        if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
            return Some(mail_dirs_under(PathBuf::from(dir)));
        }
        if let Some(home) = std::env::var_os("HOME") {
            return Some(mail_dirs_under(
                PathBuf::from(home).join(".local").join("share"),
            ));
        }
    }
    None
}

fn mail_dirs_under(base: PathBuf) -> (PathBuf, PathBuf) {
    // legacy: the second folder is the pre-rename one, read until it is moved.
    (
        base.join("excubia").join("mail"),
        base.join("herdr-notify").join("mail"),
    )
}

/// Moves the default mail folder to its new name, once. The host calls it at
/// start-up, unless `--data-dir` or `EXCUBIA_DATA_DIR` names the data.
pub fn migrate_default_dir() {
    if std::env::var_os("EXCUBIA_DATA_DIR").is_some() {
        return;
    }
    if let Some((new, old)) = mail_dirs() {
        adopt_dir(&new, &old);
    }
}
