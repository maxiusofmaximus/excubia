//! The vault of `chat.db` on the host (`docs/rfc/vault-chatdb-encryption.md`, V2).
//!
//! This is the one place that opens the chat database and decides what its file and its key say, and the one
//! place that moves it from plain to keyed. The running host (`HostVault`, behind `/v1/vault/*`) and the CLI
//! (`excubia-host vault`) both go through it, so no path opens the file on its own.

use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use anyhow::{anyhow, bail, Context, Result};
use excubia_api::{VaultControl, VaultRefusal, VaultStatus};
use excubia_chat::{
    db_state, migrate_plain_to_encrypted, open_keyed, ChatError, ChatStore, DbState, BACKUP_NAME,
};
#[cfg(windows)]
use excubia_persist::credential::CredentialManager;
use excubia_persist::credential::SecretStore;
use excubia_persist::vault::{EntropyPool, PhraseError, VaultKey, VaultKeyStore, SAMPLES_NEEDED};
use excubia_persist::RandomError;

use crate::paths;

/// What the host says when the chat is encrypted and its key is gone. The text is the owner's instruction:
/// keep it exact.
pub const MISSING_KEY: &str = "host: el historial está cifrado y no encuentro la clave (Administrador de credenciales: excubia/chat-db-key). Recupérala con «excubia-host vault recover» y tu frase de 24 palabras.";

/// What the host says when the stored key does not open the encrypted file. The file is not touched.
pub const REJECTED_KEY: &str = "host: la clave de la bóveda no abre chat.db; no toco el archivo. Recupérala con «excubia-host vault recover» y tu frase de 24 palabras.";

/// One mouse sample on the wire: `i32` x, `i32` y, `u64` micros, little-endian.
const RECORD_BYTES: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("the chat database is already protected")]
    AlreadyProtected,
    #[error("chat.db is not protected: there is no vault to recover")]
    NotEncrypted,
    #[error("there is no chat.db to open")]
    NoDatabase,
    #[error("there is no plain backup to finish")]
    NoBackup,
    #[error("not enough mouse samples to make a key")]
    NotEnoughSamples,
    #[error("the samples are not packed 16-byte records")]
    BadSamples,
    #[error("the recovery phrase does not open chat.db")]
    PhraseRejected,
    #[error("the recovery phrase: {0}")]
    Phrase(#[from] PhraseError),
    #[error("the vault key store: {0}")]
    KeyStore(io::Error),
    #[error("chat.db: {0}")]
    Chat(#[from] ChatError),
    #[error("chat.db: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Random(#[from] RandomError),
}

/// The key store of the running host: the Credential Manager.
#[cfg(windows)]
pub fn keys() -> VaultKeyStore<CredentialManager> {
    VaultKeyStore::credential_manager()
}

/// Off Windows there is no Credential Manager. The store reads as empty and refuses to write: a plain chat opens
/// as it always did, and an encrypted one is refused with `MISSING_KEY`.
#[cfg(not(windows))]
pub fn keys() -> VaultKeyStore<NoKeyring> {
    VaultKeyStore::new(NoKeyring)
}

#[cfg(not(windows))]
pub struct NoKeyring;

#[cfg(not(windows))]
impl SecretStore for NoKeyring {
    fn read(&self) -> io::Result<Option<Vec<u8>>> {
        Ok(None)
    }

    fn write(&self, _value: &[u8]) -> io::Result<()> {
        Err(io::Error::other(
            "the vault needs the Windows Credential Manager",
        ))
    }

    fn delete(&self) -> io::Result<()> {
        Ok(())
    }
}

/// Opens `chat.db` for the host, whatever its file and its key say:
/// - no file, or a plain one with no key: opened in plain text, as before (a missing file is created plain; the
///   desktop protects it later);
/// - plain with a key (an init that stopped, or a move that did not finish): moved to keyed first, the plain copy
///   kept under `vault-backup`, then opened;
/// - keyed with its key: opened; keyed without a key, or with a key that does not open it: refused, file untouched.
pub fn open_chat_store<S: SecretStore>(
    data_dir: &Path,
    keys: &VaultKeyStore<S>,
) -> Result<ChatStore> {
    let path = paths::chat_db_path(data_dir);
    let state = db_state(&path).context("read chat.db")?;
    let key = keys.load().context("read the vault key")?;
    match (state, key) {
        (DbState::Encrypted, None) => bail!(MISSING_KEY),
        (DbState::Encrypted, Some(key)) => keyed(&path, &key),
        (DbState::Plain, Some(key)) => {
            migrate_plain_to_encrypted(&path, &key, &paths::vault_backup_dir(data_dir))
                .context("move chat.db into the vault")?;
            keyed(&path, &key)
        }
        (DbState::Plain | DbState::Missing, _) => Ok(ChatStore::open(&path)?),
    }
}

fn keyed(path: &Path, key: &VaultKey) -> Result<ChatStore> {
    ChatStore::open_with_key(path, Some(key)).map_err(|error| match error {
        ChatError::VaultKeyRejected => anyhow!(REJECTED_KEY),
        other => anyhow::Error::new(other).context("open chat.db"),
    })
}

/// What the state of the vault is: protected when `chat.db` is encrypted, and whether the plain copy is kept.
pub fn status(data_dir: &Path) -> Result<VaultStatus, VaultError> {
    Ok(VaultStatus {
        protected: db_state(&paths::chat_db_path(data_dir))? == DbState::Encrypted,
        backup: backup_file(data_dir).is_file(),
        samples_needed: SAMPLES_NEEDED,
    })
}

/// Makes the key and protects the database with it, and returns the recovery phrase. `samples` are the packed mouse
/// records; `None` is the no-mouse path (the OS random source alone). `protect` does the move: the running host
/// passes its store's, the CLI passes the file's.
pub fn init<S: SecretStore>(
    data_dir: &Path,
    keys: &VaultKeyStore<S>,
    samples: Option<&[u8]>,
    protect: impl FnOnce(&VaultKey) -> Result<(), VaultError>,
) -> Result<String, VaultError> {
    let path = paths::chat_db_path(data_dir);
    match db_state(&path)? {
        DbState::Encrypted => return Err(VaultError::AlreadyProtected),
        DbState::Missing => return Err(VaultError::NoDatabase),
        DbState::Plain => {}
    }
    let pool = match samples {
        Some(samples) => pool_of(samples)?,
        None => EntropyPool::new(),
    };
    if samples.is_some() && !pool.is_enough() {
        return Err(VaultError::NotEnoughSamples);
    }
    let key = VaultKey::generate(&pool)?;
    // The phrase is made before anything is stored: a key is never kept without the words that give it back.
    let phrase = key.to_phrase()?.to_string();
    keys.save(&key).map_err(VaultError::KeyStore)?;
    if let Err(error) = protect(&key) {
        // The key goes back only when nothing encrypted can need it. An unreadable header keeps it.
        if !matches!(db_state(&path), Ok(DbState::Encrypted)) {
            if let Err(forget) = keys.delete() {
                eprintln!("host: vault: no pude quitar la clave nueva: {forget}");
            }
        }
        return Err(error);
    }
    Ok(phrase)
}

/// The move for a database that no store has open (the CLI, with the host stopped).
pub fn protect_file(data_dir: &Path, key: &VaultKey) -> Result<(), VaultError> {
    migrate_plain_to_encrypted(
        &paths::chat_db_path(data_dir),
        key,
        &paths::vault_backup_dir(data_dir),
    )?;
    Ok(())
}

/// Reads a recovery phrase and, only if it opens the encrypted `chat.db`, keeps its key in the store.
pub fn recover<S: SecretStore>(
    data_dir: &Path,
    keys: &VaultKeyStore<S>,
    phrase: &str,
) -> Result<(), VaultError> {
    let key = VaultKey::from_phrase(phrase)?;
    let path = paths::chat_db_path(data_dir);
    match db_state(&path)? {
        DbState::Encrypted => {}
        DbState::Plain => return Err(VaultError::NotEncrypted),
        DbState::Missing => return Err(VaultError::NoDatabase),
    }
    match open_keyed(&path, &key) {
        Ok(_connection) => {}
        Err(ChatError::VaultKeyRejected) => return Err(VaultError::PhraseRejected),
        Err(other) => return Err(other.into()),
    }
    keys.save(&key).map_err(VaultError::KeyStore)
}

/// Removes the plain copy that the migration kept. Each file is overwritten with zeros first; that is best effort,
/// since a journaling or flash disk may keep the old blocks.
pub fn finish(data_dir: &Path) -> Result<(), VaultError> {
    if !backup_file(data_dir).is_file() {
        return Err(VaultError::NoBackup);
    }
    shred_dir(&paths::vault_backup_dir(data_dir))?;
    Ok(())
}

fn backup_file(data_dir: &Path) -> PathBuf {
    paths::vault_backup_dir(data_dir).join(BACKUP_NAME)
}

fn shred_dir(dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let len = fs::metadata(&path)?.len();
        let mut file = OpenOptions::new().write(true).open(&path)?;
        io::copy(&mut io::repeat(0).take(len), &mut file)?;
        file.sync_all()?;
        drop(file);
        fs::remove_file(&path)?;
    }
    fs::remove_dir(dir)
}

/// The pool the packed mouse records make. The pool decides which samples count.
fn pool_of(samples: &[u8]) -> Result<EntropyPool, VaultError> {
    let (records, rest) = samples.as_chunks::<RECORD_BYTES>();
    if !rest.is_empty() {
        return Err(VaultError::BadSamples);
    }
    let mut pool = EntropyPool::new();
    for record in records {
        let x = i32::from_le_bytes([record[0], record[1], record[2], record[3]]);
        let y = i32::from_le_bytes([record[4], record[5], record[6], record[7]]);
        let micros = u64::from_le_bytes([
            record[8], record[9], record[10], record[11], record[12], record[13], record[14],
            record[15],
        ]);
        pool.add_sample(x, y, micros);
    }
    Ok(pool)
}

/// The vault as the running host holds it: its store, its key store and one change at a time.
pub struct HostVault<S: SecretStore> {
    data_dir: PathBuf,
    keys: VaultKeyStore<S>,
    chat: Arc<ChatStore>,
    /// The state check and the change of a vault call must not interleave with another call.
    turn: Mutex<()>,
}

impl<S: SecretStore> HostVault<S> {
    pub fn new(data_dir: &Path, keys: VaultKeyStore<S>, chat: Arc<ChatStore>) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            keys,
            chat,
            turn: Mutex::new(()),
        }
    }

    fn turn(&self) -> MutexGuard<'_, ()> {
        self.turn.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<S: SecretStore + 'static> VaultControl for HostVault<S> {
    fn status(&self) -> Result<VaultStatus, VaultRefusal> {
        status(&self.data_dir).map_err(report)
    }

    fn init(&self, samples: Vec<u8>) -> Result<String, VaultRefusal> {
        let _turn = self.turn();
        let backup = paths::vault_backup_dir(&self.data_dir);
        init(&self.data_dir, &self.keys, Some(&samples), |key| {
            self.chat
                .protect(key, &backup)
                .map(drop)
                .map_err(VaultError::from)
        })
        .map_err(report)
    }

    fn finish(&self) -> Result<(), VaultRefusal> {
        let _turn = self.turn();
        finish(&self.data_dir).map_err(report)
    }
}

/// Logs the detail on stderr (never a secret) and gives the API its stable answer.
fn report(error: VaultError) -> VaultRefusal {
    eprintln!("host: vault: {error}");
    match error {
        VaultError::AlreadyProtected => VaultRefusal::AlreadyProtected,
        VaultError::NoBackup => VaultRefusal::NoBackup,
        VaultError::NotEnoughSamples => VaultRefusal::NotEnoughSamples,
        VaultError::BadSamples => VaultRefusal::BadSamples,
        VaultError::KeyStore(_) => VaultRefusal::KeyStoreFailed,
        VaultError::Chat(_) => VaultRefusal::MigrationFailed,
        VaultError::NotEncrypted
        | VaultError::NoDatabase
        | VaultError::PhraseRejected
        | VaultError::Phrase(_)
        | VaultError::Io(_)
        | VaultError::Random(_) => VaultRefusal::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_come_in_whole_records() {
        assert!(matches!(pool_of(&[0u8; 17]), Err(VaultError::BadSamples)));
        assert_eq!(pool_of(&[0u8; 32]).expect("two records").samples(), 1);
    }
}
