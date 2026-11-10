//! `host mail login|accounts|logout|enable-send|disable-send` (MAIL1C §3, MAIL2A):
//! signing mailboxes in and out of this host from the terminal, and granting or
//! revoking the separate consent that lets an approved draft be sent. Nothing
//! printed here carries a token.
//!
//! The data directory holds `mail.toml` (the app registration's `client_id`)
//! and the encrypted accounts under `mail/accounts`.

use std::io::Write;
use std::path::{Path, PathBuf};

use excubia_core_mail::auth::{poll_device_login, start_device_login, start_send_consent, Tenant};
use excubia_core_mail::graph::fetch_upn;
use excubia_core_mail::{
    Account, AccountStore, HttpTransport, MailConfig, MailError, Redacted, ReqwestTransport,
};

use crate::paths;

pub(crate) const CONFIG_FILE: &str = "mail.toml";
pub const ACCOUNTS_DIR: &str = "mail/accounts";
pub(crate) const CLIENT_ID_ENV: &str = "EXCUBIA_MAIL_CLIENT_ID";

#[derive(Debug)]
pub enum MailAction {
    Login {
        account: String,
        tenant: Tenant,
        upn: Option<String>,
    },
    Accounts,
    Logout {
        account: String,
    },
    EnableSend {
        account: String,
    },
    DisableSend {
        account: String,
    },
}

/// A parsed `host mail ...` line. `None` means the default data directory.
#[derive(Debug)]
pub struct MailInvocation {
    pub action: MailAction,
    pub data_dir: Option<PathBuf>,
}

/// Everything a mail action reaches out to, passed in so tests can script it.
pub struct MailDeps<'a> {
    pub transport: &'a dyn HttpTransport,
    pub store: &'a AccountStore,
    pub client_id: Option<&'a str>,
}

pub fn parse(mut args: impl Iterator<Item = String>) -> Result<MailInvocation, String> {
    let verb = args
        .next()
        .ok_or("mail needs one of: login, accounts, logout, enable-send, disable-send")?;
    let mut flags = Flags::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--account" => flags.account = Some(value(&mut args, "--account")?),
            "--tenant" => flags.tenant = Some(value(&mut args, "--tenant")?),
            "--upn" => flags.upn = Some(value(&mut args, "--upn")?),
            "--data-dir" => flags.data_dir = Some(PathBuf::from(value(&mut args, "--data-dir")?)),
            other if !other.starts_with("--") && flags.label.is_none() => {
                flags.label = Some(other.to_owned());
            }
            other => return Err(format!("unknown argument for mail {verb}: {other}")),
        }
    }
    let data_dir = flags.data_dir.clone();
    let action = match verb.as_str() {
        "login" => MailAction::Login {
            account: required(flags.account.take(), "--account")?,
            tenant: tenant(&required(flags.tenant.take(), "--tenant")?)?,
            upn: flags.upn.take(),
        },
        "accounts" => MailAction::Accounts,
        "logout" => MailAction::Logout {
            account: required(flags.label.take().or(flags.account.take()), "a label")?,
        },
        "enable-send" => MailAction::EnableSend {
            account: required(flags.label.take().or(flags.account.take()), "a label")?,
        },
        "disable-send" => MailAction::DisableSend {
            account: required(flags.label.take().or(flags.account.take()), "a label")?,
        },
        other => return Err(format!("unknown mail command: {other}")),
    };
    Ok(MailInvocation { action, data_dir })
}

#[derive(Default)]
struct Flags {
    account: Option<String>,
    tenant: Option<String>,
    upn: Option<String>,
    data_dir: Option<PathBuf>,
    label: Option<String>,
}

fn value(args: &mut impl Iterator<Item = String>, name: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{name} needs a value"))
}

fn required(value: Option<String>, name: &str) -> Result<String, String> {
    value.ok_or_else(|| format!("mail needs {name}"))
}

fn tenant(value: &str) -> Result<Tenant, String> {
    Tenant::parse(value).map_err(|error| error.to_string())
}

/// Runs one action. Output goes to `out`; secrets never do.
pub async fn run(
    action: MailAction,
    deps: &MailDeps<'_>,
    out: &mut dyn Write,
) -> Result<(), MailError> {
    match action {
        MailAction::Login {
            account,
            tenant,
            upn,
        } => login(deps, &account, tenant, upn, out).await,
        MailAction::Accounts => accounts(deps.store, out),
        MailAction::Logout { account } => logout(deps.store, &account, out),
        MailAction::EnableSend { account } => enable_send(deps, &account, out).await,
        MailAction::DisableSend { account } => disable_send(deps.store, &account, out),
    }
}

/// The second device login: the owner signs in again with the send scopes, and
/// the send tokens join the account. The login tokens are not touched.
async fn enable_send(
    deps: &MailDeps<'_>,
    account: &str,
    out: &mut dyn Write,
) -> Result<(), MailError> {
    let client_id = deps.client_id.ok_or(MailError::NotConfigured)?;
    let tenant = saved(deps.store, account)?.tenant;
    let device = start_send_consent(deps.transport, client_id, account, tenant).await?;
    say(
        out,
        format!(
            "Concede el envío: abre {} e introduce el código {} (caduca en {} s).",
            device.verification_uri, device.user_code, device.expires_in
        ),
    )?;
    let tokens = poll_device_login(deps.transport, client_id, tenant, &device).await?;
    let current = saved(deps.store, account)?;
    deps.store.save(&current.with_send(tokens))?;
    say(out, format!("Envío activado para {account}."))
}

fn disable_send(store: &AccountStore, account: &str, out: &mut dyn Write) -> Result<(), MailError> {
    let Some(current) = store.load(account)? else {
        return say(out, format!("No hay cuenta {account}."));
    };
    store.save(&current.without_send())?;
    say(out, format!("Envío desactivado para {account}."))
}

fn saved(store: &AccountStore, account: &str) -> Result<Account, MailError> {
    store
        .load(account)?
        .ok_or_else(|| MailError::InvalidInput(Redacted::new("no account with that label")))
}

async fn login(
    deps: &MailDeps<'_>,
    account: &str,
    tenant: Tenant,
    upn: Option<String>,
    out: &mut dyn Write,
) -> Result<(), MailError> {
    let client_id = deps.client_id.ok_or(MailError::NotConfigured)?;
    let device = start_device_login(deps.transport, client_id, account, tenant).await?;
    say(
        out,
        format!(
            "Abre {} e introduce el código {} (caduca en {} s).",
            device.verification_uri, device.user_code, device.expires_in
        ),
    )?;
    let tokens = poll_device_login(deps.transport, client_id, tenant, &device).await?;
    let upn = match upn {
        Some(upn) => upn,
        None => fetch_upn(deps.transport, tokens.access_token()).await?,
    };
    deps.store
        .save(&Account::new(account, tenant, upn.clone(), tokens))?;
    say(out, format!("Cuenta {account} guardada ({upn})."))
}

fn accounts(store: &AccountStore, out: &mut dyn Write) -> Result<(), MailError> {
    for account in store.all()? {
        let line = format!(
            "{}\t{}\t{}",
            account.label,
            account.tenant.as_str(),
            account.upn
        );
        say(out, line)?;
    }
    Ok(())
}

fn logout(store: &AccountStore, account: &str, out: &mut dyn Write) -> Result<(), MailError> {
    if store.load(account)?.is_none() {
        return say(out, format!("No hay cuenta {account}."));
    }
    store.remove(account)?;
    say(out, format!("Cuenta {account} olvidada."))
}

fn say(out: &mut dyn Write, line: String) -> Result<(), MailError> {
    writeln!(out, "{line}").map_err(|error| MailError::Store(Redacted::new(error.to_string())))
}

/// The app registration's client id: `[mail] client_id` in `mail.toml`, or the
/// environment. `None` when neither is set.
pub fn client_id(config: &Path, env_client_id: Option<&str>) -> Result<Option<String>, MailError> {
    let raw = match std::fs::read_to_string(config) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(MailError::InvalidConfig(Redacted::new(error.to_string()))),
    };
    match MailConfig::from_toml(&raw, env_client_id) {
        Ok(config) => Ok(Some(config.client_id)),
        Err(MailError::NotConfigured) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Runs an invocation of the real binary: Graph, the platform key and the
/// data directory of this host.
pub async fn main(invocation: MailInvocation, out: &mut dyn Write) -> Result<(), MailError> {
    let data_dir = match invocation.data_dir {
        Some(dir) => dir,
        None => paths::default_data_dir()
            .map_err(|error| MailError::InvalidConfig(Redacted::new(error)))?,
    };
    let store = AccountStore::open_default(data_dir.join(ACCOUNTS_DIR))?;
    let env_client_id = std::env::var(CLIENT_ID_ENV).ok();
    let client_id = client_id(&data_dir.join(CONFIG_FILE), env_client_id.as_deref())?;
    let transport = ReqwestTransport::new()?;
    let deps = MailDeps {
        transport: &transport,
        store: &store,
        client_id: client_id.as_deref(),
    };
    run(invocation.action, &deps, out).await
}
