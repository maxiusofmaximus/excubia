//! The host's side of the mail routes (MAIL1D §1, MAIL2A). [`MailLogins`] answers
//! the API's [`MailActions`]. Every refusal leaves with the stable code of the
//! error and a fixed message, so no text from Microsoft reaches a client.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use excubia_api::{
    DraftEdit, MailAccount, MailActions, MailDraft, MailDraftAttachment, MailFailure,
    MailLoginStart, MailLoginStatus, SendAnswer, SendFailure, SendStatus,
};
use excubia_chat::{Approval, ChatStore};
use excubia_mail::auth::Tenant;
use excubia_mail::config::DEFAULT_SENDS_PER_HOUR;
use excubia_mail::{AccountStore, DraftReview, MailConfig, MailError, Redacted, ReqwestTransport};

use crate::mail_api::MailLogins;
use crate::mail_cli::{client_id, ACCOUNTS_DIR, CLIENT_ID_ENV, CONFIG_FILE};
use crate::mail_send::{MailSends, SendPolicy, SendStep, DEFAULT_WINDOW};
use crate::mail_tools::open_ledger;

/// The mail surface of a host: the logins, the sending consent and the sends,
/// over one account store, one transport, one ledger and the sign-in cards of
/// the chat. With no registration every login is refused with
/// `mail_not_configured`. A login never outlives its process, so the cards a
/// previous run left behind are taken away here, and a send a restart caught in
/// its window is cancelled here, with an audit line for each.
pub fn open(data_dir: &Path, chat: Arc<ChatStore>) -> Result<MailLogins, MailError> {
    let (store, client_id, transport) = parts(data_dir)?;
    let ledger = open_ledger(data_dir)?;
    chat.close_all_mail_cards()
        .map_err(|error| MailError::Store(Redacted::new(error.to_string())))?;
    let per_hour = MailConfig::load(&data_dir.join(CONFIG_FILE))
        .map_or(DEFAULT_SENDS_PER_HOUR, |config| config.max_sends_per_hour);
    let transport = Arc::new(transport);
    let store = Arc::new(store);
    let sends = MailSends::new(
        transport.clone(),
        store.clone(),
        client_id.clone(),
        ledger,
        &data_dir.join(ACCOUNTS_DIR),
        chat.clone(),
        SendPolicy {
            per_hour,
            window: DEFAULT_WINDOW,
        },
    );
    sends.recover()?;
    sends.watch();
    Ok(MailLogins::with_sends(
        transport, store, client_id, chat, sends,
    ))
}

/// The pieces every mail surface of a host shares: the encrypted account store,
/// the app registration (none when the host is not configured) and the transport.
pub(crate) fn parts(
    data_dir: &Path,
) -> Result<(AccountStore, Option<String>, ReqwestTransport), MailError> {
    let store = AccountStore::open_default(data_dir.join(ACCOUNTS_DIR))?;
    let env_client_id = std::env::var(CLIENT_ID_ENV).ok();
    let client_id = client_id(&data_dir.join(CONFIG_FILE), env_client_id.as_deref())?;
    let transport = ReqwestTransport::new()?;
    Ok((store, client_id, transport))
}

#[async_trait]
impl MailActions for MailLogins {
    fn approval_decided(&self, approval: &Approval) {
        self.decided(approval);
    }

    async fn login_start(
        &self,
        account: &str,
        tenant: &str,
        conversation_id: Option<&str>,
    ) -> Result<MailLoginStart, MailFailure> {
        let Ok(tenant) = Tenant::parse(tenant) else {
            return Err(refusal(MailError::InvalidInput(Redacted::new(
                "unknown tenant",
            ))));
        };
        let start = self
            .start(account, tenant, conversation_id)
            .await
            .map_err(refusal)?;
        Ok(login_fields(start))
    }

    fn login_status(&self, account: &str) -> Result<MailLoginStatus, MailFailure> {
        self.status_of(account).map_err(refusal)
    }

    fn accounts(&self) -> Result<Vec<MailAccount>, MailFailure> {
        let accounts = MailLogins::accounts(self).map_err(refusal)?;
        Ok(accounts
            .into_iter()
            .map(|account| MailAccount {
                label: account.label,
                tenant: account.tenant.as_str().to_owned(),
                upn: account.upn,
                send_enabled: account.send_enabled,
            })
            .collect())
    }

    async fn send(
        &self,
        account: &str,
        draft_id: &str,
        approval_id: Option<&str>,
    ) -> Result<SendAnswer, MailFailure> {
        let sends = self.sends().map_err(refusal)?;
        match sends
            .send(account, draft_id, approval_id)
            .await
            .map_err(refusal)?
        {
            SendStep::Awaiting {
                approval_id,
                expires_in,
            } => Ok(SendAnswer::Awaiting {
                approval_id,
                expires_in,
            }),
            SendStep::Scheduled {
                send_id,
                window_seconds,
            } => Ok(SendAnswer::Scheduled {
                send_id,
                window_seconds,
            }),
        }
    }

    fn send_status(&self, send_id: &str) -> Result<SendStatus, MailFailure> {
        let row = self
            .sends()
            .map_err(refusal)?
            .status(send_id)
            .map_err(refusal)?;
        Ok(SendStatus {
            state: row.state.public(),
            failure: row.failure.map(|failure| SendFailure {
                code: failure.code.as_str(),
                retry: failure.retry.as_str(),
            }),
        })
    }

    fn send_cancel(&self, send_id: &str) -> Result<&'static str, MailFailure> {
        self.sends()
            .map_err(refusal)?
            .cancel(send_id)
            .map_err(refusal)
    }

    async fn send_enable_start(&self, account: &str) -> Result<MailLoginStart, MailFailure> {
        let start = self.start_send(account).await.map_err(refusal)?;
        Ok(login_fields(start))
    }

    fn send_enable_status(&self, account: &str) -> Result<MailLoginStatus, MailFailure> {
        self.send_status_of(account).map_err(refusal)
    }

    fn send_disable(&self, account: &str) -> Result<(), MailFailure> {
        self.disable_send(account).map_err(refusal)
    }

    async fn draft_review(&self, account: &str, draft_id: &str) -> Result<MailDraft, MailFailure> {
        let review = self
            .sends()
            .map_err(refusal)?
            .review(account, draft_id)
            .await
            .map_err(refusal)?;
        Ok(draft_of(review))
    }

    async fn draft_update(
        &self,
        account: &str,
        draft_id: &str,
        edit: DraftEdit,
        device_id: &str,
    ) -> Result<MailDraft, MailFailure> {
        let review = self
            .sends()
            .map_err(refusal)?
            .update_draft(account, draft_id, edit, device_id)
            .await
            .map_err(refusal)?;
        Ok(draft_of(review))
    }
}

fn login_fields(start: crate::mail_api::LoginStart) -> MailLoginStart {
    MailLoginStart {
        login_id: start.login_id,
        user_code: start.user_code,
        verification_uri: start.verification_uri,
        expires_in: start.expires_in,
        interval: start.interval,
    }
}

fn draft_of(review: DraftReview) -> MailDraft {
    MailDraft {
        body: review.text(),
        account: review.account,
        draft_id: review.draft_id,
        from: review.from,
        to: review.to,
        cc: review.cc,
        subject: review.subject,
        content_type: review.content_type,
        attachments: review
            .attachments
            .into_iter()
            .map(|item| MailDraftAttachment {
                name: item.name,
                size: item.size,
                sha256: item.sha256,
            })
            .collect(),
    }
}

/// The failure the API carries: the error's stable code, a fixed message and,
/// for a rate limit only, the seconds to wait.
fn refusal(error: MailError) -> MailFailure {
    let retry_after = match error {
        MailError::SendRateLimited { retry_after } => Some(retry_after),
        _ => None,
    };
    let code = error.code();
    // The API message is fixed on purpose; the operator still needs Graph's own answer in the host log.
    if matches!(
        error,
        MailError::Graph { .. }
            | MailError::Transport(_)
            | MailError::BadResponse(_)
            | MailError::UploadFailed(_)
    ) {
        eprintln!("mail failure {code}: {error}");
    }
    MailFailure {
        code,
        message: message_of(code),
        retry_after,
    }
}

/// The fixed message for a stable code. It never carries what the error held,
/// which may be an address, a path or a token.
pub(crate) fn message_of(code: &str) -> &'static str {
    match code {
        "mail_not_configured" => "the mail app is not configured on this host",
        "mail_invalid_input" => "the request names an unknown login, account, draft or tenant",
        "mail_path_rejected" => "the file cannot be attached from that path",
        "attachment_not_found" => "no file of this conversation has that attachment_id",
        "mail_approval_denied" => "the operator did not allow this action; nothing was done",
        "mail_approval_unavailable" => "no operator answer could be asked for this file",
        "mail_content_changed" => "the file changed while it was being read; nothing was sent",
        "mail_upload_failed" => "the file could not be uploaded; the draft is unchanged",
        "mail_unknown_draft" => "the draft is not one this host created",
        "mail_not_found" => "the message is not in this mailbox",
        "mail_send_not_enabled" => "sending is not enabled for this account; the owner must grant it",
        "mail_approval_stale" => {
            "the draft changed after it was approved, or the approval was already used; nothing was sent"
        }
        "mail_approval_pending" => "the owner has not answered the send approval yet",
        "mail_draft_refused" => "el borrador fue rechazado; cámbialo para pedirlo de nuevo",
        "mail_send_rate_limited" => "too many mails were sent from this account in the last hour",
        "mail_move_stale" => "the moves changed after the owner approved them; nothing was moved",
        "mail_move_already_applied" => "the owner already applied this batch; nothing was moved again",
        "mail_move_refused" => "the owner refused this batch of moves; change it to ask again",
        _ => "the mail request failed on this host",
    }
}
