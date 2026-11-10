//! The local record of each move of a message (F2A): what moved, from which folder
//! to which, and whether it was undone. It keeps no subject, sender or body. The
//! verdict, decider and approval fields stay empty until the moves that need them
//! exist.
//!
//! One JSON object per line, beside the audit log in the accounts folder. Marking a
//! record undone rewrites the file, so appends and rewrites are serialised and the
//! file is owner-only.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use excubia_core_persist::write_secret_atomic;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::audit::unix_now;
use crate::error::MailError;
use crate::mover::Moved;
use crate::private::open_append;
use crate::redact::Redacted;

const FILE_NAME: &str = "actions.jsonl";

/// One move of a message, or the undo of one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailActionRecord {
    pub id: String,
    /// Unix seconds.
    pub ts: u64,
    pub account: String,
    pub tool: String,
    pub message_id_before: String,
    /// Graph gives a moved message a new id, so this is the one to undo from.
    pub message_id_after: String,
    pub from_folder: String,
    pub to_folder: String,
    pub verdict_level: Option<String>,
    pub decider_kind: Option<String>,
    pub approval_id: Option<String>,
    pub undone: bool,
}

impl MailActionRecord {
    /// A record of a move the caller just made. The optional fields start empty.
    pub fn new(
        account: &str,
        tool: &str,
        message_id_before: &str,
        moved: &Moved,
        to_folder: &str,
    ) -> Self {
        Self {
            id: Ulid::generate().to_string(),
            ts: unix_now(),
            account: account.to_owned(),
            tool: tool.to_owned(),
            message_id_before: message_id_before.to_owned(),
            message_id_after: moved.id.clone(),
            from_folder: moved.from_folder.clone(),
            to_folder: to_folder.to_owned(),
            verdict_level: None,
            decider_kind: None,
            approval_id: None,
            undone: false,
        }
    }
}

pub struct ActionLog {
    path: PathBuf,
    /// Keeps a rewrite from losing a line appended meanwhile.
    writes: Mutex<()>,
}

impl ActionLog {
    pub fn new(accounts_dir: &Path) -> Self {
        Self {
            path: accounts_dir.join(FILE_NAME),
            writes: Mutex::new(()),
        }
    }

    pub fn append(&self, record: &MailActionRecord) -> Result<(), MailError> {
        let mut line = serde_json::to_vec(record).map_err(log_error)?;
        line.push(b'\n');
        let _guard = self.lock();
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(log_error)?;
        }
        let mut file = open_append(&self.path).map_err(log_error)?;
        file.write_all(&line).map_err(log_error)
    }

    /// The newest records of `account`, newest first, at most `n`.
    pub fn list_recent(&self, account: &str, n: usize) -> Result<Vec<MailActionRecord>, MailError> {
        Ok(self
            .read_all()?
            .into_iter()
            .rev()
            .filter(|record| record.account == account)
            .take(n)
            .collect())
    }

    pub fn find(&self, account: &str, id: &str) -> Result<Option<MailActionRecord>, MailError> {
        Ok(self
            .read_all()?
            .into_iter()
            .find(|record| record.account == account && record.id == id))
    }

    /// The newest record whose moved message has this id.
    pub fn find_by_after_id(
        &self,
        account: &str,
        message_id: &str,
    ) -> Result<Option<MailActionRecord>, MailError> {
        Ok(self
            .read_all()?
            .into_iter()
            .rev()
            .find(|record| record.account == account && record.message_id_after == message_id))
    }

    /// Marks a record as undone and rewrites the file. Fails for an unknown id.
    pub fn mark_undone(&self, id: &str) -> Result<(), MailError> {
        let _guard = self.lock();
        let mut records = self.read_all()?;
        let record = records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or(MailError::ActionNotFound)?;
        record.undone = true;
        let mut bytes = Vec::new();
        for record in &records {
            serde_json::to_writer(&mut bytes, record).map_err(log_error)?;
            bytes.push(b'\n');
        }
        write_secret_atomic(&self.path, &bytes).map_err(log_error)
    }

    /// Every record in file order. A missing file is an empty log.
    fn read_all(&self) -> Result<Vec<MailActionRecord>, MailError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(log_error(error)),
        };
        text.lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(line).map_err(log_error))
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.writes.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn log_error(reason: impl std::fmt::Display) -> MailError {
    MailError::Store(Redacted::new(format!(
        "the action log cannot be used: {reason}"
    )))
}
