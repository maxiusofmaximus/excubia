//! Sending an approved draft (MAIL2A). A human asks; the owner answers the
//! approval in the inbox; the send waits out a cancel window; then one POST
//! reaches Graph with the send token.
//!
//! The order is the safety. A send exists in the ledger only once the owner
//! approved this exact draft. The draft is read again when the window ends, and
//! a change since approval ends the send as stale. A cancel, or a restart, stops
//! a send that has not reached Graph. Every outcome is audited with the digest
//! and the recipient domains, never with the content.
//!
//! The owner's questions after they are asked, and their audit lines, live in
//! `sweep`.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use excubia_core_chat::{
    new_ulid, Approval, ApprovalKind, ApprovalStatus, ChatError, ChatStore, NewApproval,
};
use excubia_core_mail::{
    check_budget, domain_of, Account, AccountStore, AuditLog, Closing, DraftReview, HttpTransport,
    Ledger, MailError, Redacted, SendQuestion, SendResult, SendRow, SendState, SendView,
    RATE_WINDOW_SECONDS,
};
use sweep::report;

mod edit;
mod graph;
mod sweep;

/// The question the owner answers in the inbox. Never delegable (MAIL2A).
pub const ACTION: &str = "mail.send";

/// How long the owner has to answer before the question expires.
pub const SEND_TTL_SECONDS: u32 = 120;

/// The cancel window of a send, unless the host was built with another.
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(10);

/// The limits a host sends under: how many sends an account may start in an
/// hour, and how long a send waits to be cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendPolicy {
    pub per_hour: u32,
    pub window: Duration,
}

/// What the API learns from a send request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendStep {
    /// The owner has not answered yet. Ask again with this id.
    Awaiting {
        approval_id: String,
        expires_in: u64,
    },
    /// Approved: the send leaves when the window ends, unless cancelled.
    Scheduled {
        send_id: String,
        window_seconds: u64,
    },
}

/// The sends of a host. Cloning shares the same state, the ledger and the timers.
#[derive(Clone)]
pub struct MailSends {
    inner: Arc<Inner>,
}

struct Inner {
    transport: Arc<dyn HttpTransport>,
    store: Arc<AccountStore>,
    client_id: Option<String>,
    ledger: Ledger,
    audit: AuditLog,
    chat: Arc<ChatStore>,
    policy: SendPolicy,
}

impl MailSends {
    /// `ledger` is the one the drafts use, so ownership and sends see the same
    /// rows. `accounts_dir` is where the audit file lives.
    pub fn new(
        transport: Arc<dyn HttpTransport>,
        store: Arc<AccountStore>,
        client_id: Option<String>,
        ledger: Ledger,
        accounts_dir: &Path,
        chat: Arc<ChatStore>,
        policy: SendPolicy,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                transport,
                store,
                client_id,
                ledger,
                audit: AuditLog::new(accounts_dir),
                chat,
                policy,
            }),
        }
    }

    /// The ledger the drafts and the sends share.
    pub fn ledger(&self) -> Ledger {
        self.inner.ledger.clone()
    }

    /// Asks for the approval of sending `draft_id`, or, with `approval_id`,
    /// checks that approval and schedules the send if the owner said yes.
    pub async fn send(
        &self,
        account: &str,
        draft_id: &str,
        approval_id: Option<&str>,
    ) -> Result<SendStep, MailError> {
        let saved = self.saved(account)?;
        if !saved.send_enabled() {
            return Err(MailError::SendNotEnabled);
        }
        self.check_budget(account)?;
        let view = self.read_draft(&saved, draft_id).await?;
        let approval = match approval_id {
            None => self.open_approval(&view)?,
            Some(id) => self.inner.chat.approval(id).map_err(chat_error)?,
        };
        match approval.status {
            ApprovalStatus::Pending => Ok(SendStep::Awaiting {
                approval_id: approval.id,
                expires_in: u64::from(SEND_TTL_SECONDS),
            }),
            ApprovalStatus::Denied => self.refuse(&approval, Closing::Denied),
            ApprovalStatus::Expired => self.refuse(&approval, Closing::Expired),
            ApprovalStatus::Consumed => self.refuse(&approval, Closing::Stale),
            ApprovalStatus::Approved => {
                if view.digest() != approval.action_digest {
                    // The draft moved since the owner approved it: that approval
                    // is spent for good, and the owner must be asked again.
                    let _ = self
                        .inner
                        .chat
                        .consume_for_host(&approval.id, &approval.action_digest);
                    return self.refuse(&approval, Closing::Stale);
                }
                self.schedule(&view, &approval)
            }
        }
    }

    /// The draft as the operator reads it before approving a send. A read only:
    /// no consent, no budget, and nothing changes.
    pub async fn review(&self, account: &str, draft_id: &str) -> Result<DraftReview, MailError> {
        let saved = self.saved(account)?;
        self.review_draft(&saved, draft_id).await
    }

    /// The send as the ledger holds it. Its public state is `state.public()`, and a
    /// `failed` one carries why it failed.
    pub fn status(&self, send_id: &str) -> Result<SendRow, MailError> {
        self.row(send_id)
    }

    /// Cancels a send that is still in its window. Once it is sending or sent,
    /// nothing changes and the state it is in is the answer.
    pub fn cancel(&self, send_id: &str) -> Result<&'static str, MailError> {
        let row = self.row(send_id)?;
        if self
            .inner
            .ledger
            .move_send(send_id, SendState::PendingSend, SendState::Cancelled)?
        {
            self.audit_row(&row, SendResult::Cancelled)?;
            return Ok(SendState::Cancelled.public());
        }
        Ok(self.row(send_id)?.state.public())
    }

    fn row(&self, send_id: &str) -> Result<SendRow, MailError> {
        self.inner
            .ledger
            .send_row(send_id)?
            .ok_or_else(|| MailError::InvalidInput(Redacted::new("unknown send")))
    }

    fn saved(&self, account: &str) -> Result<Account, MailError> {
        self.inner
            .store
            .load(account)?
            .ok_or_else(|| MailError::InvalidInput(Redacted::new("no account with that label")))
    }

    fn client_id(&self) -> Result<&str, MailError> {
        self.inner
            .client_id
            .as_deref()
            .ok_or(MailError::NotConfigured)
    }

    /// Refuses a send before the owner is asked when the hourly budget is spent.
    fn check_budget(&self, account: &str) -> Result<(), MailError> {
        let now = now();
        let times = self
            .inner
            .ledger
            .budget_times(account, now - RATE_WINDOW_SECONDS)?;
        check_budget(&times, now, self.inner.policy.per_hour)
    }

    /// Asks the owner, and records the question so its outcome can be audited.
    /// The chat refuses a digest it already holds, which is the one-approval-per-
    /// content rule: a refused draft must change to be asked again.
    fn open_approval(&self, view: &SendView) -> Result<Approval, MailError> {
        let digest = view.digest();
        let question = NewApproval {
            conversation_id: None,
            seat_id: None,
            agent_label: None,
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Tool,
            tool: ACTION.to_owned(),
            summary: format!(
                "Enviar «{}» a {} destinatario(s)",
                view.subject,
                view.to.len() + view.cc.len()
            ),
            cwd: None,
            paths: Vec::new(),
            risk: None,
            action_digest: digest.clone(),
            payload_redacted: Some(view.details()),
            ttl_seconds: SEND_TTL_SECONDS,
        };
        let approval = self
            .inner
            .chat
            .create_mail_send_approval(question)
            .map_err(chat_error)?;
        self.inner.ledger.open_question(&SendQuestion {
            approval_id: approval.id.clone(),
            account: view.account.clone(),
            draft_id: view.draft_id.clone(),
            digest,
            domains: domains_of(view),
        })?;
        Ok(approval)
    }

    /// Takes the approval for this send and puts the send in its window. The
    /// approval is burned first, so two calls cannot both schedule one send.
    fn schedule(&self, view: &SendView, approval: &Approval) -> Result<SendStep, MailError> {
        if self
            .inner
            .chat
            .consume_for_host(&approval.id, &approval.action_digest)
            .is_err()
        {
            return self.refuse(approval, Closing::Stale);
        }
        self.inner
            .ledger
            .close_question(&approval.id, Closing::Scheduled)?;
        let window = self.inner.policy.window;
        let row = SendRow {
            send_id: new_ulid(),
            draft_id: view.draft_id.clone(),
            account: view.account.clone(),
            state: SendState::PendingSend,
            digest: approval.action_digest.clone(),
            approval_id: approval.id.clone(),
            domains: domains_of(view),
            created_at: now(),
            failure: None,
        };
        self.inner.ledger.insert_send(&row)?;
        let sends = self.clone();
        let send_id = row.send_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(window).await;
            sends.finish(&send_id).await;
        });
        Ok(SendStep::Scheduled {
            send_id: row.send_id,
            window_seconds: window.as_secs(),
        })
    }

    /// Settles the question as refused and says why to the caller. Nothing is
    /// sent. A question already settled writes no second line.
    fn refuse(&self, approval: &Approval, closing: Closing) -> Result<SendStep, MailError> {
        self.settle(&approval.id, closing)?;
        Err(match closing {
            Closing::Denied | Closing::Expired => MailError::ApprovalDenied,
            Closing::Scheduled | Closing::Stale => MailError::ApprovalStale,
        })
    }

    /// Runs when the window ends: claims the send, reads the draft again, sends
    /// it once and records the outcome. A cancel that won the race leaves nothing to do.
    async fn finish(&self, send_id: &str) {
        let claimed =
            self.inner
                .ledger
                .move_send(send_id, SendState::PendingSend, SendState::Sending);
        if !matches!(claimed, Ok(true)) {
            return;
        }
        let Ok(Some(row)) = self.inner.ledger.send_row(send_id) else {
            return;
        };
        let (saved, result) = match self.dispatch(&row).await {
            Ok(()) => (
                self.inner
                    .ledger
                    .move_send(send_id, SendState::Sending, SendState::Sent),
                SendResult::Sent,
            ),
            Err(failure) => (
                self.inner
                    .ledger
                    .fail_send(send_id, SendState::Sending, failure),
                failure.audit_result(),
            ),
        };
        if let Err(error) = saved {
            report("the send state could not be saved", &error);
        }
        if let Err(error) = self.audit_row(&row, result) {
            report("the send could not be audited", &error);
        }
    }
}

/// The domain of each recipient, `to` first, as the audit keeps it.
fn domains_of(view: &SendView) -> Vec<String> {
    view.recipient_addresses()
        .iter()
        .map(|address| domain_of(address))
        .collect()
}

fn chat_error(error: ChatError) -> MailError {
    match error {
        ChatError::ApprovalDigestReused => MailError::ApprovalStale,
        ChatError::ApprovalDigestRefused => MailError::DraftRefused,
        ChatError::NoSuchApproval => MailError::InvalidInput(Redacted::new("unknown approval")),
        other => MailError::ApprovalUnavailable(Redacted::new(other.to_string())),
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}
