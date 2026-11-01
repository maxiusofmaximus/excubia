//! The owner's questions once they are asked (MAIL2A2): the sweep that settles a
//! refused or expired one, the recovery after a restart, the settling of a
//! question whose draft changed, and the audit line each outcome leaves. A
//! question closes once, so each line is written once, whichever path sees the
//! outcome first.

use std::time::Duration;

use hn_core_chat::ApprovalStatus;
use hn_core_mail::{Closing, MailError, SendFailure, SendRecord, SendResult, SendRow, SendState};
use tokio::sync::broadcast::error::RecvError;

use super::MailSends;

/// How often the sweep runs when no chat change wakes it: an expiry moves no
/// frame by itself, so something has to look.
pub const SWEEP_EVERY: Duration = Duration::from_secs(1);

impl MailSends {
    /// Ends what a previous run left behind. A send still in its window never
    /// reached Graph, so it is cancelled; one caught mid-POST is failed as
    /// `interrupted`, since Graph may or may not have taken it. Both are audited.
    pub fn recover(&self) -> Result<(), MailError> {
        for row in self.inner.ledger.unfinished_sends()? {
            let (moved, result) = match row.state {
                SendState::PendingSend => (
                    self.inner
                        .ledger
                        .move_send(&row.send_id, row.state, SendState::Cancelled)?,
                    SendResult::Cancelled,
                ),
                SendState::Sending => (
                    self.inner.ledger.fail_send(
                        &row.send_id,
                        row.state,
                        SendFailure::INTERRUPTED,
                    )?,
                    SendResult::Failed,
                ),
                _ => continue,
            };
            if moved {
                self.audit_row(&row, result)?;
            }
        }
        Ok(())
    }

    /// Runs the sweep for as long as the host lives: at once, after every chat
    /// change, and at least every [`SWEEP_EVERY`]. The task ends when the chat
    /// closes. Needs a Tokio runtime.
    pub fn watch(&self) {
        let sends = self.clone();
        let mut frames = self.inner.chat.subscribe();
        tokio::spawn(async move {
            loop {
                sends.sweep();
                if matches!(
                    tokio::time::timeout(SWEEP_EVERY, frames.recv()).await,
                    Ok(Err(RecvError::Closed))
                ) {
                    break;
                }
            }
        });
    }

    /// Settles the questions the owner refused or let expire, and audits each
    /// one once. A failure is reported and the next sweep tries again.
    pub fn sweep(&self) {
        let questions = match self.inner.ledger.open_questions() {
            Ok(questions) => questions,
            Err(error) => {
                report("the open questions could not be read", &error);
                return;
            }
        };
        for question in questions {
            let Ok(approval) = self.inner.chat.approval(&question.approval_id) else {
                continue;
            };
            let closing = match approval.status {
                ApprovalStatus::Denied => Closing::Denied,
                ApprovalStatus::Expired => Closing::Expired,
                _ => continue,
            };
            if let Err(error) = self.settle(&approval.id, closing) {
                report("a send question could not be audited", &error);
            }
        }
    }

    /// The draft changed while the owner's question about it is open: that
    /// question is stale now and audited now. The owner's answer, whenever it
    /// comes, cannot send the draft that was read.
    pub fn draft_changed(&self, account: &str, draft_id: &str) -> Result<(), MailError> {
        for question in self.inner.ledger.open_questions()? {
            if question.account == account && question.draft_id == draft_id {
                self.settle(&question.approval_id, Closing::Stale)?;
            }
        }
        Ok(())
    }

    /// Closes the question of `approval_id` and writes its audit line, once. A
    /// scheduled send leaves no line here: its own rows are audited.
    pub(super) fn settle(&self, approval_id: &str, closing: Closing) -> Result<(), MailError> {
        let Some(question) = self.inner.ledger.close_question(approval_id, closing)? else {
            return Ok(());
        };
        let Some(result) = audit_result(closing) else {
            return Ok(());
        };
        let approver = self
            .inner
            .chat
            .approval(approval_id)
            .ok()
            .and_then(|a| a.decided_by);
        self.inner.audit.append(&SendRecord::new(
            &question.account,
            &question.draft_id,
            &question.digest,
            approver.as_deref(),
            result,
            question.domains,
        ))
    }

    pub(super) fn audit_row(&self, row: &SendRow, result: SendResult) -> Result<(), MailError> {
        let approver = self
            .inner
            .chat
            .approval(&row.approval_id)
            .ok()
            .and_then(|a| a.decided_by);
        self.inner.audit.append(&SendRecord::new(
            &row.account,
            &row.draft_id,
            &row.digest,
            approver.as_deref(),
            result,
            row.domains.clone(),
        ))
    }
}

/// The audit line a closing writes. A scheduled send writes none of its own.
fn audit_result(closing: Closing) -> Option<SendResult> {
    match closing {
        Closing::Scheduled => None,
        Closing::Denied => Some(SendResult::Denied),
        Closing::Expired => Some(SendResult::Expired),
        Closing::Stale => Some(SendResult::Stale),
    }
}

/// A line for the operator's log: what failed and the stable code, never the
/// error's text, since that can carry an address or a token. A Graph refusal also
/// names its HTTP status and Graph's own error code, which are fixed words.
fn note(what: &str, error: &MailError) -> String {
    match error {
        MailError::Graph { status, code, .. } => {
            format!("mail: {what} ({}, HTTP {status}, {code})", error.code())
        }
        _ => format!("mail: {what} ({})", error.code()),
    }
}

pub(super) fn report(what: &str, error: &MailError) {
    eprintln!("{}", note(what, error));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_names_the_code_and_never_the_detail() {
        let error = MailError::InvalidInput(hn_core_mail::Redacted::new("ana@ejemplo.com"));
        let line = note("the send could not be audited", &error);
        assert_eq!(
            line,
            "mail: the send could not be audited (mail_invalid_input)"
        );
        assert!(!line.contains("ejemplo"), "{line}");
    }
}
