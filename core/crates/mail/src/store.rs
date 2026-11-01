//! Accounts at rest: one AES-256-GCM file per label, with the master key in
//! the Windows Credential Manager (reusing `hn-core-persist`'s store, so this
//! crate keeps its `forbid(unsafe_code)`) or in an owner-only `0600` file on
//! other platforms.
//!
//! The plaintext of an account file never touches disk: [`AccountStore::save`]
//! encrypts the JSON first and the envelope holds only a nonce and ciphertext.

use std::fmt;
use std::path::PathBuf;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::auth::{Tenant, Tokens};
use crate::error::MailError;
use crate::redact::{Redacted, Secret};

const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const FORMAT_VERSION: u8 = 1;
#[cfg(not(windows))]
const KEY_FILE: &str = "master.key";
const ACCOUNT_SUFFIX: &str = ".json";
const MAX_LABEL_BYTES: usize = 64;

/// One signed-in mailbox. `send` holds the tokens of the send consent: present
/// only after the owner grants `Mail.Send` (MAIL2A), and removed by revoking it.
#[derive(Clone)]
pub struct Account {
    pub label: String,
    pub tenant: Tenant,
    pub upn: String,
    pub tokens: Tokens,
    pub send: Option<Tokens>,
}

impl Account {
    pub fn new(
        label: impl Into<String>,
        tenant: Tenant,
        upn: impl Into<String>,
        tokens: Tokens,
    ) -> Self {
        Self {
            label: label.into(),
            tenant,
            upn: upn.into(),
            tokens,
            send: None,
        }
    }

    /// The same account with rotated login tokens; the send consent is kept.
    pub fn renewed(&self, tokens: Tokens) -> Self {
        Self {
            tokens,
            ..self.clone()
        }
    }

    /// The same account with the send consent granted.
    pub fn with_send(&self, send: Tokens) -> Self {
        Self {
            send: Some(send),
            ..self.clone()
        }
    }

    /// The same account with the send consent revoked (its tokens forgotten).
    pub fn without_send(&self) -> Self {
        Self {
            send: None,
            ..self.clone()
        }
    }

    pub fn send_enabled(&self) -> bool {
        self.send.is_some()
    }
}

impl fmt::Debug for Account {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Account")
            .field("label", &self.label)
            .field("tenant", &self.tenant)
            .field("upn", &self.upn)
            .field("tokens", &self.tokens)
            .field("send", &self.send)
            .finish()
    }
}

/// Where the AES key comes from. Implementations never log the key.
pub trait KeySource: Send + Sync {
    fn master_key(&self) -> Result<[u8; KEY_BYTES], MailError>;
}

/// An owner-only key file, for platforms without a credential store and for
/// tests. The file is created on first use and read unchanged afterwards.
pub struct FileKey {
    path: PathBuf,
}

impl FileKey {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl KeySource for FileKey {
    fn master_key(&self) -> Result<[u8; KEY_BYTES], MailError> {
        match std::fs::read(&self.path) {
            Ok(raw) => key_from_bytes(&raw),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let key = random_bytes::<KEY_BYTES>()?;
                hn_core_persist::write_secret_atomic(&self.path, &key).map_err(store_error)?;
                Ok(key)
            }
            Err(error) => Err(store_error(error)),
        }
    }
}

#[cfg(windows)]
pub use windows_key::CredentialKey;

#[cfg(windows)]
mod windows_key {
    use hn_core_persist::credential::{CredentialManager, SecretStore};

    use super::{key_from_bytes, random_bytes, store_error, KeySource, KEY_BYTES};
    use crate::error::MailError;

    const TARGET: &str = "herdr-notify/mail-master-key";

    /// The Windows secret store that holds the master key.
    pub struct CredentialKey {
        store: CredentialManager,
    }

    impl CredentialKey {
        pub fn new() -> Self {
            Self {
                store: CredentialManager::new(TARGET),
            }
        }
    }

    impl Default for CredentialKey {
        fn default() -> Self {
            Self::new()
        }
    }

    impl KeySource for CredentialKey {
        fn master_key(&self) -> Result<[u8; KEY_BYTES], MailError> {
            if let Some(raw) = self.store.read().map_err(store_error)? {
                return key_from_bytes(&raw);
            }
            let key = random_bytes::<KEY_BYTES>()?;
            self.store.write(&key).map_err(store_error)?;
            Ok(key)
        }
    }
}

/// The account files of one host. The key is loaded once at `open`.
pub struct AccountStore {
    dir: PathBuf,
    cipher: Aes256Gcm,
}

impl AccountStore {
    pub fn open(dir: impl Into<PathBuf>, key_source: &dyn KeySource) -> Result<Self, MailError> {
        let dir = dir.into();
        let key = key_source.master_key()?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|_| MailError::Store(Redacted::new("the master key has an invalid length")))?;
        std::fs::create_dir_all(&dir).map_err(store_error)?;
        Ok(Self { dir, cipher })
    }

    /// Open with the platform key: Credential Manager on Windows, a `0600`
    /// file next to the accounts elsewhere.
    pub fn open_default(dir: impl Into<PathBuf>) -> Result<Self, MailError> {
        let dir = dir.into();
        #[cfg(windows)]
        {
            Self::open(dir, &CredentialKey::new())
        }
        #[cfg(not(windows))]
        {
            let key = FileKey::new(dir.join(KEY_FILE));
            Self::open(dir, &key)
        }
    }

    pub fn save(&self, account: &Account) -> Result<(), MailError> {
        let path = self.path(&account.label)?;
        let stored = StoredAccount::from(account);
        let plaintext = serde_json::to_vec(&stored).map_err(store_error)?;
        let nonce = Nonce::from(random_bytes::<NONCE_BYTES>()?);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_slice())
            .map_err(|_| MailError::Store(Redacted::new("the account could not be encrypted")))?;
        let envelope = SealedFile {
            v: FORMAT_VERSION,
            nonce: STANDARD.encode(nonce),
            data: STANDARD.encode(ciphertext),
        };
        let json = serde_json::to_vec(&envelope).map_err(store_error)?;
        hn_core_persist::write_secret_atomic(&path, &json).map_err(store_error)
    }

    /// `Ok(None)` means this host has no account under that label.
    pub fn load(&self, label: &str) -> Result<Option<Account>, MailError> {
        let path = self.path(label)?;
        let raw = match std::fs::read(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(store_error(error)),
        };
        let envelope: SealedFile = serde_json::from_slice(&raw).map_err(|_| {
            MailError::Store(Redacted::new("the account file is not a valid envelope"))
        })?;
        if envelope.v != FORMAT_VERSION {
            return Err(MailError::Store(Redacted::new(
                "the account file uses an unsupported format version",
            )));
        }
        let nonce = STANDARD
            .decode(&envelope.nonce)
            .map_err(|_| MailError::Store(Redacted::new("the account nonce is not base64")))?;
        let nonce: [u8; NONCE_BYTES] = nonce.try_into().map_err(|_| {
            MailError::Store(Redacted::new("the account nonce has an invalid length"))
        })?;
        let ciphertext = STANDARD
            .decode(&envelope.data)
            .map_err(|_| MailError::Store(Redacted::new("the account data is not base64")))?;
        let plaintext = self
            .cipher
            .decrypt(&Nonce::from(nonce), ciphertext.as_slice())
            .map_err(|_| {
                MailError::Store(Redacted::new(
                    "the account file cannot be decrypted with this master key",
                ))
            })?;
        let stored: StoredAccount = serde_json::from_slice(&plaintext).map_err(|_| {
            MailError::Store(Redacted::new("the decrypted account is not valid JSON"))
        })?;
        if stored.label != label {
            return Err(MailError::Store(Redacted::new(
                "the account file names a different label",
            )));
        }
        Ok(Some(stored.into_account()))
    }

    /// The labels of the accounts on this host, sorted. No token is read.
    pub fn labels(&self) -> Result<Vec<String>, MailError> {
        let mut labels = Vec::new();
        for entry in std::fs::read_dir(&self.dir).map_err(store_error)? {
            let name = entry.map_err(store_error)?.file_name();
            if let Some(label) = name.to_string_lossy().strip_suffix(ACCOUNT_SUFFIX) {
                labels.push(label.to_owned());
            }
        }
        labels.sort();
        Ok(labels)
    }

    /// Every account on this host, in label order.
    pub fn all(&self) -> Result<Vec<Account>, MailError> {
        let mut accounts = Vec::new();
        for label in self.labels()? {
            accounts.extend(self.load(&label)?);
        }
        Ok(accounts)
    }

    /// Forgets the account under `label`. An absent label is not an error.
    pub fn remove(&self, label: &str) -> Result<(), MailError> {
        let path = self.path(label)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(store_error(error)),
        }
    }

    fn path(&self, label: &str) -> Result<PathBuf, MailError> {
        validate_label(label)?;
        Ok(self.dir.join(format!("{label}{ACCOUNT_SUFFIX}")))
    }
}

fn validate_label(label: &str) -> Result<(), MailError> {
    let valid = !label.is_empty()
        && label.len() <= MAX_LABEL_BYTES
        && !label.starts_with('.')
        && label
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '@'));
    if valid {
        Ok(())
    } else {
        Err(MailError::InvalidInput(Redacted::new(
            "an account label is 1-64 ASCII letters, digits, '-', '_', '.' or '@' and does not start with '.'",
        )))
    }
}

#[derive(Serialize, Deserialize)]
struct StoredAccount {
    label: String,
    tenant: Tenant,
    upn: String,
    #[serde(flatten)]
    login: StoredTokens,
    /// Absent in files written before MAIL2A, which means no send consent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    send: Option<StoredTokens>,
}

#[derive(Serialize, Deserialize)]
struct StoredTokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

impl From<&Tokens> for StoredTokens {
    fn from(tokens: &Tokens) -> Self {
        Self {
            access_token: tokens.access_token().to_owned(),
            refresh_token: tokens.refresh_token().map(str::to_owned),
            expires_in: tokens.expires_in(),
        }
    }
}

impl StoredTokens {
    fn into_tokens(self) -> Tokens {
        Tokens::new(
            self.access_token,
            self.refresh_token.map(Secret::new),
            self.expires_in,
        )
    }
}

impl From<&Account> for StoredAccount {
    fn from(account: &Account) -> Self {
        Self {
            label: account.label.clone(),
            tenant: account.tenant,
            upn: account.upn.clone(),
            login: StoredTokens::from(&account.tokens),
            send: account.send.as_ref().map(StoredTokens::from),
        }
    }
}

impl StoredAccount {
    fn into_account(self) -> Account {
        Account {
            label: self.label,
            tenant: self.tenant,
            upn: self.upn,
            tokens: self.login.into_tokens(),
            send: self.send.map(StoredTokens::into_tokens),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SealedFile {
    v: u8,
    nonce: String,
    data: String,
}

fn key_from_bytes(raw: &[u8]) -> Result<[u8; KEY_BYTES], MailError> {
    raw.try_into()
        .map_err(|_| MailError::Store(Redacted::new("the master key has an invalid length")))
}

fn random_bytes<const N: usize>() -> Result<[u8; N], MailError> {
    let mut bytes = [0u8; N];
    hn_core_persist::fill_random(&mut bytes).map_err(store_error)?;
    Ok(bytes)
}

fn store_error(error: impl fmt::Display) -> MailError {
    MailError::Store(Redacted::new(error.to_string()))
}
