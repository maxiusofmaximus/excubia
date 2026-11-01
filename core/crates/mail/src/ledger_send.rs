//! The send rows of the ledger (MAIL2A): one per confirmed send, created only
//! once the owner approved. A refused, expired or stale approval leaves no row,
//! only an audit line. The row holds ids, the digest, the state and the
//! recipient domains, never content or addresses.

use rusqlite::{params, OptionalExtension, Row};

use crate::error::MailError;
use crate::ledger::{now, store, Ledger};
use crate::redact::Redacted;
use crate::send::SendState;
use crate::send_failure::{FailCode, Retry, SendFailure};

/// One confirmed send as the ledger remembers it. `failure` is set only when the
/// send is `failed`; a failed row from before codes existed reads as [`SendFailure::LEGACY`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendRow {
    pub send_id: String,
    pub draft_id: String,
    pub account: String,
    pub state: SendState,
    pub digest: String,
    pub approval_id: String,
    pub domains: Vec<String>,
    pub created_at: i64,
    pub failure: Option<SendFailure>,
}

type RawRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    i64,
    Option<String>,
    Option<String>,
);

fn read_raw(row: &Row<'_>) -> rusqlite::Result<RawRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}

fn into_row(raw: RawRow) -> Result<SendRow, MailError> {
    let state = SendState::parse(&raw.3)
        .ok_or_else(|| MailError::Store(Redacted::new("the send ledger holds an unknown state")))?;
    let failure = (state == SendState::Failed).then(|| stored_failure(raw.8, raw.9));
    Ok(SendRow {
        send_id: raw.0,
        draft_id: raw.1,
        account: raw.2,
        state,
        digest: raw.4,
        approval_id: raw.5,
        domains: raw
            .6
            .split(',')
            .filter(|domain| !domain.is_empty())
            .map(str::to_owned)
            .collect(),
        created_at: raw.7,
        failure,
    })
}

/// The failure a row holds, or the conservative legacy one when a code is missing
/// or unknown: an old failed send may have reached Graph.
fn stored_failure(code: Option<String>, retry: Option<String>) -> SendFailure {
    let parsed = code
        .as_deref()
        .and_then(FailCode::parse)
        .zip(retry.as_deref().and_then(Retry::parse));
    parsed.map_or(SendFailure::LEGACY, |(code, retry)| SendFailure {
        code,
        retry,
    })
}

const COLUMNS: &str =
    "send_id, draft_id, account, state, digest, approval_id, domains, created_at, failure_code, failure_retry";

impl Ledger {
    /// Records a confirmed send. The approval id is unique, so one approval
    /// can never create two sends.
    pub fn insert_send(&self, row: &SendRow) -> Result<(), MailError> {
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO mail_send
                    (send_id, draft_id, account, state, digest, approval_id, domains,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![
                    row.send_id,
                    row.draft_id,
                    row.account,
                    row.state.as_str(),
                    row.digest,
                    row.approval_id,
                    row.domains.join(","),
                    row.created_at,
                ],
            )
            .map_err(store)?;
        Ok(())
    }

    pub fn send_row(&self, send_id: &str) -> Result<Option<SendRow>, MailError> {
        let connection = self.lock()?;
        let raw = connection
            .query_row(
                &format!("SELECT {COLUMNS} FROM mail_send WHERE send_id = ?1"),
                params![send_id],
                read_raw,
            )
            .optional()
            .map_err(store)?;
        raw.map(into_row).transpose()
    }

    /// Moves a send from `from` to `to`, only if it is still in `from`. The
    /// answer says whether this call made the move, which is how the window,
    /// the cancel and the send race without a second winner.
    pub fn move_send(
        &self,
        send_id: &str,
        from: SendState,
        to: SendState,
    ) -> Result<bool, MailError> {
        let connection = self.lock()?;
        let changed = connection
            .execute(
                "UPDATE mail_send SET state = ?3, updated_at = ?4
                 WHERE send_id = ?1 AND state = ?2",
                params![send_id, from.as_str(), to.as_str(), now()],
            )
            .map_err(store)?;
        Ok(changed == 1)
    }

    /// Moves a send from `from` to `failed` and stores why, in one `UPDATE`, only if
    /// it is still in `from`. The same race rule as [`Ledger::move_send`].
    pub fn fail_send(
        &self,
        send_id: &str,
        from: SendState,
        failure: SendFailure,
    ) -> Result<bool, MailError> {
        let connection = self.lock()?;
        let changed = connection
            .execute(
                "UPDATE mail_send
                 SET state = 'failed', failure_code = ?3, failure_retry = ?4, updated_at = ?5
                 WHERE send_id = ?1 AND state = ?2",
                params![
                    send_id,
                    from.as_str(),
                    failure.code.as_str(),
                    failure.retry.as_str(),
                    now(),
                ],
            )
            .map_err(store)?;
        Ok(changed == 1)
    }

    /// The creation times of the sends of `account` that reached the budget:
    /// everything except the cancelled ones.
    pub fn budget_times(&self, account: &str, since: i64) -> Result<Vec<i64>, MailError> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT created_at FROM mail_send
                 WHERE account = ?1 AND created_at > ?2 AND state != 'cancelled'",
            )
            .map_err(store)?;
        let rows = statement
            .query_map(params![account, since], |row| row.get(0))
            .map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }

    /// The sends that a restart left unfinished: the window and the POST.
    pub fn unfinished_sends(&self) -> Result<Vec<SendRow>, MailError> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(&format!(
                "SELECT {COLUMNS} FROM mail_send
                 WHERE state IN ('pending_send', 'sending') ORDER BY created_at"
            ))
            .map_err(store)?;
        let rows = statement.query_map([], read_raw).map_err(store)?;
        let raw: Vec<RawRow> = rows.collect::<Result<_, _>>().map_err(store)?;
        raw.into_iter().map(into_row).collect()
    }

    /// The name and SHA-256 Excubia recorded for each attachment of a draft.
    pub fn attachment_digests(&self, draft_id: &str) -> Result<Vec<(String, String)>, MailError> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT name, sha256 FROM mail_attachment WHERE draft_id = ?1
                 ORDER BY created_at, attachment_id",
            )
            .map_err(store)?;
        let rows = statement
            .query_map(params![draft_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(store)?;
        rows.collect::<Result<_, _>>().map_err(store)
    }
}
