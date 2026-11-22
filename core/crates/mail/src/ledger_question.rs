//! The questions the host asked the owner about a send (MAIL2A audit): one row
//! per question, with what an audit line needs (account, draft, digest and the
//! recipient domains). A question is closed once, by the first outcome that
//! settles it, so each one is audited at most once, whichever path sees the
//! outcome first: the sweep, a query or a restart.

use rusqlite::{params, OptionalExtension, Row};

use crate::error::MailError;
use crate::ledger::{now, store, Ledger};

/// One question asked of the owner about sending a draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendQuestion {
    pub approval_id: String,
    pub account: String,
    pub draft_id: String,
    pub digest: String,
    pub domains: Vec<String>,
}

/// How a question was settled. `Scheduled` is the owner's yes: the send row
/// takes over, so the question writes no audit line of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    Scheduled,
    Denied,
    Expired,
    Stale,
}

impl Closing {
    fn as_str(self) -> &'static str {
        match self {
            Self::Scheduled => "scheduled",
            Self::Denied => "denied",
            Self::Expired => "expired",
            Self::Stale => "stale",
        }
    }
}

const COLUMNS: &str = "approval_id, account, draft_id, digest, domains";

fn read(row: &Row<'_>) -> rusqlite::Result<SendQuestion> {
    let domains: String = row.get(4)?;
    Ok(SendQuestion {
        approval_id: row.get(0)?,
        account: row.get(1)?,
        draft_id: row.get(2)?,
        digest: row.get(3)?,
        domains: domains
            .split(',')
            .filter(|domain| !domain.is_empty())
            .map(str::to_owned)
            .collect(),
    })
}

impl Ledger {
    /// Records a question asked of the owner. The approval id is the key.
    pub fn open_question(&self, question: &SendQuestion) -> Result<(), MailError> {
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO mail_question
                    (approval_id, account, draft_id, digest, domains, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    question.approval_id,
                    question.account,
                    question.draft_id,
                    question.digest,
                    question.domains.join(","),
                    now(),
                ],
            )
            .map_err(store)?;
        Ok(())
    }

    /// Closes the question if it is still open and returns it. `None` means an
    /// earlier outcome already closed it, so the caller writes nothing. The lock
    /// covers the read and the write, so two callers cannot both close it.
    pub fn close_question(
        &self,
        approval_id: &str,
        closing: Closing,
    ) -> Result<Option<SendQuestion>, MailError> {
        let connection = self.lock()?;
        let question = connection
            .query_row(
                &format!(
                    "SELECT {COLUMNS} FROM mail_question
                     WHERE approval_id = ?1 AND closed IS NULL"
                ),
                params![approval_id],
                read,
            )
            .optional()
            .map_err(store)?;
        if question.is_some() {
            connection
                .execute(
                    "UPDATE mail_question SET closed = ?2 WHERE approval_id = ?1",
                    params![approval_id, closing.as_str()],
                )
                .map_err(store)?;
        }
        Ok(question)
    }

    /// The questions nobody has closed yet.
    pub fn open_questions(&self) -> Result<Vec<SendQuestion>, MailError> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {COLUMNS} FROM mail_question WHERE closed IS NULL"
            ))
            .map_err(store)?;
        let rows = statement.query_map([], read).map_err(store)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(store)
    }
}
