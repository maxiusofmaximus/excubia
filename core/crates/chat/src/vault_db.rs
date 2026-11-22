//! The chat database in its two states, and the one-way move from plain to keyed
//! (`docs/rfc/vault-chatdb-encryption.md`, migration).
//!
//! `chat.db` is not touched until a keyed copy has passed every check. The plain
//! file is backed up first, the copy is written with SQLCipher's own export and
//! reopened with the key, and only then is it renamed over `chat.db`.

use std::fs::{self, File};
use std::io::{self, ErrorKind, Read};
use std::path::{Path, PathBuf};

use excubia_persist::vault::VaultKey;
use rusqlite::{params, Connection, ErrorCode};

use crate::error::ChatError;

/// The first 16 bytes of every plain SQLite file.
const PLAIN_HEADER: &[u8; 16] = b"SQLite format 3\0";
pub const BACKUP_NAME: &str = "chat.db.pre-vault";
const TMP_SUFFIX: &str = ".vault-tmp";
const SIDECARS: [&str; 3] = ["-wal", "-shm", "-journal"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbState {
    /// No file, or an empty one.
    Missing,
    Plain,
    /// Anything else: a keyed file, or a file that is not a database at all.
    Encrypted,
}

/// What a finished migration left behind.
#[derive(Debug)]
pub struct Migration {
    pub tables: usize,
    pub rows: i64,
    /// The plain copy. It stays where it is until the owner removes it.
    pub backup: PathBuf,
}

/// The state of `path`, read from its first 16 bytes.
pub fn db_state(path: &Path) -> io::Result<DbState> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(DbState::Missing),
        Err(error) => return Err(error),
    };
    let mut head = Vec::with_capacity(PLAIN_HEADER.len());
    file.take(PLAIN_HEADER.len() as u64).read_to_end(&mut head)?;
    Ok(if head.is_empty() {
        DbState::Missing
    } else if head == PLAIN_HEADER {
        DbState::Plain
    } else {
        DbState::Encrypted
    })
}

/// Opens `path` and applies `key`, then proves it by reading the schema. A file the
/// key does not open is refused, never replaced; a missing file is created empty.
/// No migration runs here: that is `schema::open_with_key`'s job.
pub fn open_keyed(path: &Path, key: &VaultKey) -> Result<Connection, ChatError> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "key", key.sqlcipher_pragma().as_str())?;
    match connection.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
        row.get::<_, i64>(0)
    }) {
        Ok(_) => Ok(connection),
        Err(rusqlite::Error::SqliteFailure(failure, _))
            if failure.code == ErrorCode::NotADatabase =>
        {
            Err(ChatError::VaultKeyRejected)
        }
        Err(error) => Err(error.into()),
    }
}

/// Turns the plain `path` into a keyed one. The plain file is copied to `backup_dir`
/// first; `path` is replaced only after the keyed copy has passed every check. A
/// failure before the swap leaves `path` as it was. An attempt that died halfway
/// leaves a temporary file, which the next attempt discards.
///
/// Precondition: no other connection may write to `path` while this runs. A write
/// made after the copy would be lost by the swap. The host runs it before it opens
/// the store.
pub fn migrate_plain_to_encrypted(
    path: &Path,
    key: &VaultKey,
    backup_dir: &Path,
) -> Result<Migration, ChatError> {
    match db_state(path)? {
        DbState::Plain => {}
        DbState::Missing => return Err(ChatError::VaultMissing),
        DbState::Encrypted => return Err(ChatError::VaultAlreadyEncrypted),
    }
    let tmp = with_suffix(path, TMP_SUFFIX);
    clear(&tmp)?;
    let backup = back_up(path, backup_dir)?;
    let report = build_copy(path, &tmp, key)
        .and_then(|report| swap_in(path, &tmp).map(|()| report))
        .inspect_err(|_| {
            let _ = clear(&tmp);
        })?;
    Ok(Migration {
        tables: report.tables,
        rows: report.rows,
        backup,
    })
}

/// Makes the plain file whole and copies it to `backup_dir`. The checkpoint folds
/// the WAL into the file first, so the copy holds every committed page.
fn back_up(path: &Path, backup_dir: &Path) -> Result<PathBuf, ChatError> {
    {
        let plain = Connection::open(path)?;
        let busy: i64 =
            plain.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
        if busy != 0 {
            return Err(ChatError::VaultMigration("the checkpoint: the database is busy"));
        }
        if integrity(&plain)? != "ok" {
            return Err(ChatError::VaultMigration("the integrity check of chat.db"));
        }
    }
    fs::create_dir_all(backup_dir)?;
    let backup = backup_dir.join(BACKUP_NAME);
    copy_file(path, &backup)?;
    for suffix in SIDECARS {
        let side = with_suffix(path, suffix);
        if side.exists() {
            copy_file(&side, &with_suffix(&backup, suffix))?;
        }
    }
    let same_size = fs::metadata(path)?.len() == fs::metadata(&backup)?.len();
    if !same_size || db_state(&backup)? != DbState::Plain {
        return Err(ChatError::VaultMigration("the check of the backup"));
    }
    Ok(backup)
}

/// What the copy must reproduce: the schema version and the rows of every table.
#[derive(PartialEq, Eq)]
struct Snapshot {
    version: i64,
    counts: Vec<(String, i64)>,
}

struct Report {
    tables: usize,
    rows: i64,
}

fn build_copy(path: &Path, tmp: &Path, key: &VaultKey) -> Result<Report, ChatError> {
    let source = export(path, tmp, key)?;
    verify_copy(tmp, key, &source)
}

/// Writes the keyed copy with SQLCipher's own export, and returns what the plain
/// file held before it.
fn export(path: &Path, tmp: &Path, key: &VaultKey) -> Result<Snapshot, ChatError> {
    let plain = Connection::open(path)?;
    let source = snapshot(&plain)?;
    let target = tmp
        .to_str()
        .ok_or(ChatError::VaultMigration("the path of the copy is not UTF-8"))?;
    let pragma = key.sqlcipher_pragma();
    plain.execute(
        "ATTACH DATABASE ?1 AS enc KEY ?2",
        params![target, pragma.as_str()],
    )?;
    plain.query_row("SELECT sqlcipher_export('enc')", [], |_| Ok(()))?;
    plain.pragma_update(Some("enc"), "user_version", source.version)?;
    plain.execute_batch("DETACH DATABASE enc")?;
    Ok(source)
}

/// Reopens the copy with the key and compares it with the plain file it came from.
fn verify_copy(tmp: &Path, key: &VaultKey, source: &Snapshot) -> Result<Report, ChatError> {
    let copy = open_keyed(tmp, key)?;
    if integrity(&copy)? != "ok" {
        return Err(ChatError::VaultMigration("the integrity check of the copy"));
    }
    let copied = snapshot(&copy)?;
    if copied.version != source.version {
        return Err(ChatError::VaultMigration("the schema version of the copy"));
    }
    if copied != *source {
        return Err(ChatError::VaultMigration("the row counts of the copy"));
    }
    Ok(Report {
        tables: copied.counts.len(),
        rows: copied.counts.iter().map(|(_, rows)| *rows).sum(),
    })
}

/// The one step that replaces `chat.db`. The plain file's WAL was folded into it
/// before the copy and is already in the backup, so it is removed first: a leftover
/// would be replayed onto the keyed file.
fn swap_in(path: &Path, tmp: &Path) -> Result<(), ChatError> {
    clear_sidecars(path)?;
    clear_sidecars(tmp)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn snapshot(connection: &Connection) -> Result<Snapshot, ChatError> {
    let version = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    Ok(Snapshot {
        version,
        counts: table_counts(connection)?,
    })
}

/// Row counts of every ordinary table, FTS shadow tables included. The FTS table
/// itself is skipped: its index is rebuilt when the database is opened.
fn table_counts(connection: &Connection) -> Result<Vec<(String, i64)>, ChatError> {
    let names: Vec<String> = {
        let mut statement = connection.prepare(
            "SELECT name FROM sqlite_master
              WHERE type = 'table' AND COALESCE(sql, '') NOT LIKE 'CREATE VIRTUAL TABLE%'
              ORDER BY name",
        )?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut counts = Vec::with_capacity(names.len());
    for name in names {
        let sql = format!("SELECT count(*) FROM \"{}\"", name.replace('"', "\"\""));
        let rows: i64 = connection.query_row(&sql, [], |row| row.get(0))?;
        counts.push((name, rows));
    }
    Ok(counts)
}

fn integrity(connection: &Connection) -> Result<String, ChatError> {
    connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(ChatError::from)
}

/// Copies through a staged file, so an interrupted copy never reads as a backup.
fn copy_file(from: &Path, to: &Path) -> Result<(), ChatError> {
    let staged = with_suffix(to, ".part");
    let copied = fs::copy(from, &staged).and_then(|_| fs::rename(&staged, to));
    if let Err(error) = copied {
        let _ = fs::remove_file(&staged);
        return Err(error.into());
    }
    Ok(())
}

/// Removes `path` and its sidecars. A file that is not there is not an error.
fn clear(path: &Path) -> Result<(), ChatError> {
    remove_if_present(path)?;
    clear_sidecars(path)
}

fn clear_sidecars(path: &Path) -> Result<(), ChatError> {
    for suffix in SIDECARS {
        remove_if_present(&with_suffix(path, suffix))?;
    }
    Ok(())
}

fn remove_if_present(path: &Path) -> Result<(), ChatError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
#[path = "vault_db_tests.rs"]
mod tests;
