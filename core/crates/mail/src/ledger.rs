//! The ledger of what this host created: its drafts, and the attachments Graph
//! accepted on them. It keeps ownership and idempotency across restarts.
//!
//! It holds ids, content hashes, names, sizes, the account and the time. It
//! never holds mail content, subjects, addresses or upload URLs; a send records
//! only the domain of each recipient. Rows older than [`RETENTION_SECONDS`] are
//! purged when the ledger opens.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::draft::AttachmentMeta;
use crate::error::MailError;
use crate::redact::Redacted;

/// An identical draft within this window is the same draft, not a new one.
pub(crate) const IDEMPOTENCY_SECONDS: i64 = 15 * 60;
/// How long the ledger remembers a draft and its attachments.
pub const RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS mail_draft (
    draft_id     TEXT PRIMARY KEY,
    account      TEXT NOT NULL,
    content_hash TEXT,
    body_sha256  TEXT,
    created_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS mail_draft_content ON mail_draft (account, content_hash);
CREATE TABLE IF NOT EXISTS mail_attachment (
    attachment_id TEXT PRIMARY KEY,
    draft_id      TEXT NOT NULL REFERENCES mail_draft (draft_id) ON DELETE CASCADE,
    account       TEXT NOT NULL,
    sha256        TEXT NOT NULL,
    name          TEXT NOT NULL,
    size          INTEGER NOT NULL,
    created_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS mail_attachment_content ON mail_attachment (draft_id, sha256, name);
CREATE TABLE IF NOT EXISTS mail_send (
    send_id     TEXT PRIMARY KEY,
    draft_id    TEXT NOT NULL,
    account     TEXT NOT NULL,
    state       TEXT NOT NULL,
    digest      TEXT NOT NULL,
    approval_id TEXT NOT NULL UNIQUE,
    domains     TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS mail_send_account ON mail_send (account, created_at);
CREATE TABLE IF NOT EXISTS mail_question (
    approval_id TEXT PRIMARY KEY,
    account     TEXT NOT NULL,
    draft_id    TEXT NOT NULL,
    digest      TEXT NOT NULL,
    domains     TEXT NOT NULL,
    closed      TEXT,
    created_at  INTEGER NOT NULL
);
";

/// Cloned handles share one connection, so the drafts service and the send
/// service see the same rows.
#[derive(Clone)]
pub struct Ledger {
    connection: Arc<Mutex<Connection>>,
}

impl Ledger {
    /// Opens (or creates) the ledger file and purges what is past retention.
    pub fn open(path: &Path) -> Result<Self, MailError> {
        Self::from_connection(Connection::open(path).map_err(store)?)
    }

    /// A ledger that lives as long as the value. Tests use it where no restart is needed.
    pub fn in_memory() -> Result<Self, MailError> {
        Self::from_connection(Connection::open_in_memory().map_err(store)?)
    }

    fn from_connection(connection: Connection) -> Result<Self, MailError> {
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(store)?;
        connection.execute_batch(SCHEMA).map_err(store)?;
        add_column(&connection, "mail_draft", "body_sha256")?;
        add_column(&connection, "mail_send", "failure_code")?;
        add_column(&connection, "mail_send", "failure_retry")?;
        let ledger = Self {
            connection: Arc::new(Mutex::new(connection)),
        };
        ledger.purge(now())?;
        Ok(ledger)
    }

    pub(crate) fn lock(&self) -> Result<MutexGuard<'_, Connection>, MailError> {
        self.connection
            .lock()
            .map_err(|_| MailError::Store(Redacted::new("the mail ledger lock is poisoned")))
    }

    /// Whether `account` created `draft_id` and still remembers it.
    pub fn owns(&self, account: &str, draft_id: &str) -> Result<bool, MailError> {
        let connection = self.lock()?;
        let found: Option<i64> = connection
            .query_row(
                "SELECT 1 FROM mail_draft WHERE draft_id = ?1 AND account = ?2",
                params![draft_id, account],
                |row| row.get(0),
            )
            .optional()
            .map_err(store)?;
        Ok(found.is_some())
    }

    /// The draft `account` created from this content within the window, if any.
    pub(crate) fn draft_for_content(
        &self,
        account: &str,
        content_hash: &str,
    ) -> Result<Option<String>, MailError> {
        let connection = self.lock()?;
        connection
            .query_row(
                "SELECT draft_id FROM mail_draft
                 WHERE account = ?1 AND content_hash = ?2 AND created_at >= ?3",
                params![account, content_hash, now() - IDEMPOTENCY_SECONDS],
                |row| row.get(0),
            )
            .optional()
            .map_err(store)
    }

    pub(crate) fn remember_draft(
        &self,
        account: &str,
        draft_id: &str,
        content_hash: &str,
    ) -> Result<(), MailError> {
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO mail_draft (draft_id, account, content_hash, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![draft_id, account, content_hash, now()],
            )
            .map_err(store)?;
        Ok(())
    }

    /// The draft's text changed, so its old content can no longer return it.
    pub(crate) fn forget_content(&self, draft_id: &str) -> Result<(), MailError> {
        let connection = self.lock()?;
        connection
            .execute(
                "UPDATE mail_draft SET content_hash = NULL WHERE draft_id = ?1",
                params![draft_id],
            )
            .map_err(store)?;
        Ok(())
    }

    /// Remembers the body this host wrote to the draft, as a SHA-256: a read
    /// drops the hard space it added only when Graph holds that body plus the
    /// space, so a hard space the owner wrote is never taken for its own.
    pub(crate) fn remember_body(&self, draft_id: &str, body: &str) -> Result<(), MailError> {
        let connection = self.lock()?;
        connection
            .execute(
                "UPDATE mail_draft SET body_sha256 = ?1 WHERE draft_id = ?2",
                params![digest_hex(body.as_bytes()), draft_id],
            )
            .map_err(store)?;
        Ok(())
    }

    /// The digest of the body this host last wrote, if it still remembers one.
    pub(crate) fn body_sha256(&self, draft_id: &str) -> Result<Option<String>, MailError> {
        let connection = self.lock()?;
        connection
            .query_row(
                "SELECT body_sha256 FROM mail_draft WHERE draft_id = ?1",
                params![draft_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(store)
            .map(Option::flatten)
    }

    /// The attachment already accepted for these bytes under this name, if any.
    pub(crate) fn attachment(
        &self,
        draft_id: &str,
        sha256: &str,
        name: &str,
    ) -> Result<Option<AttachmentMeta>, MailError> {
        let connection = self.lock()?;
        connection
            .query_row(
                "SELECT attachment_id, name, size FROM mail_attachment
                 WHERE draft_id = ?1 AND sha256 = ?2 AND name = ?3
                 ORDER BY created_at, attachment_id LIMIT 1",
                params![draft_id, sha256, name],
                |row| {
                    Ok(AttachmentMeta {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        size: row.get::<_, i64>(2)? as u64,
                    })
                },
            )
            .optional()
            .map_err(store)
    }

    /// Records an attachment Graph accepted, with the digest of its bytes.
    pub(crate) fn remember_attachment(
        &self,
        account: &str,
        draft_id: &str,
        sha256: &str,
        meta: &AttachmentMeta,
    ) -> Result<(), MailError> {
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO mail_attachment
                    (attachment_id, draft_id, account, sha256, name, size, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    meta.id,
                    draft_id,
                    account,
                    sha256,
                    meta.name,
                    meta.size as i64,
                    now()
                ],
            )
            .map_err(store)?;
        Ok(())
    }

    /// The names of the attachments on a draft, in the order they were accepted.
    pub(crate) fn attachment_names(&self, draft_id: &str) -> Result<Vec<String>, MailError> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT name FROM mail_attachment WHERE draft_id = ?1
                 ORDER BY created_at, attachment_id",
            )
            .map_err(store)?;
        let rows = statement
            .query_map(params![draft_id], |row| row.get(0))
            .map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }

    /// Deletes drafts, and their attachments, created before the retention window.
    fn purge(&self, now: i64) -> Result<usize, MailError> {
        let connection = self.lock()?;
        connection
            .execute(
                "DELETE FROM mail_draft WHERE created_at < ?1",
                params![now - RETENTION_SECONDS],
            )
            .map_err(store)
    }
}

/// SHA-256 as lowercase hex: the digest of a payload or of a file's bytes.
pub(crate) fn digest_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Adds a TEXT column a ledger from an earlier build lacks. Old rows keep NULL.
fn add_column(connection: &Connection, table: &str, column: &str) -> Result<(), MailError> {
    let present: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            params![table, column],
            |row| row.get(0),
        )
        .map_err(store)?;
    if present == 0 {
        connection
            .execute(&format!("ALTER TABLE {table} ADD COLUMN {column} TEXT"), [])
            .map_err(store)?;
    }
    Ok(())
}

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

pub(crate) fn store(err: rusqlite::Error) -> MailError {
    MailError::Store(Redacted::new(format!("the mail ledger failed: {err}")))
}
