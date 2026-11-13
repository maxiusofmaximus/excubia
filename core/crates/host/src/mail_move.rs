//! Proposing a batch of moves (F2b2). The host reads each message, works out its
//! verdict and asks the operator once for the whole batch. Nothing moves here. The
//! payload the operator decides on carries the sender and the subject the host read,
//! never a body, and the reason Alfred gave for each message.

use excubia_chat::{ApprovalKind, ChatError, NewApproval, SeatActor};
use excubia_mail::received::one_line;
use excubia_mail::triage::verdict;
use excubia_mail::{batch_digest, MailError, MoveDest, ReadRecord, Redacted};
use excubia_seat::{MoveFolder, MoveProposal, ProposedMove};
use serde_json::{json, Value};

use crate::mail_tools::MailTools;

/// The action the batch is filed under. Also the `tool` of its question.
pub const ACTION: &str = "mail.move";

/// How long the operator has to decide a batch before it expires.
const TTL_SECONDS: u32 = 1800;

/// The subject the approval shows; the sender and the subject are the host's, not Alfred's.
const SUBJECT_CHARS: usize = 120;

/// What Alfred is told after a proposal: nothing has moved yet.
pub(crate) const PENDING_NOTICE: &str = "Pendiente de la decisión del Master. No muevas nada hasta que te diga que decidió; entonces llama a mail.move_apply.";

impl MailTools {
    /// Reads every message of the batch, in order, then asks the operator once. A read
    /// that fails rejects the whole batch before anything is created. Each read is
    /// audited by its id.
    pub(crate) async fn propose(
        &self,
        actor: &SeatActor,
        proposal: &MoveProposal,
    ) -> Result<Value, MailError> {
        let access = self.session(&proposal.account).await?;
        let client = self.client(&access, &proposal.account);
        let mut rows = Vec::with_capacity(proposal.items.len());
        for item in &proposal.items {
            let message = client.read_message(&item.message_id).await?;
            self.audit.append(&ReadRecord::new(
                &proposal.account,
                "mail.move_propose",
                Some(&item.message_id),
            ))?;
            rows.push(json!({
                "message_id": item.message_id,
                "dest": item.folder.graph_name(),
                "reason": item.reason,
                "from_address": message.from_address,
                "subject": one_line(&message.subject, SUBJECT_CHARS),
                "level": verdict(&message.checks).level.as_str(),
            }));
        }
        let digest = batch_digest(
            &proposal.account,
            proposal
                .items
                .iter()
                .map(|item| (item.message_id.as_str(), dest_of(item.folder))),
        );
        let question = NewApproval {
            conversation_id: (!actor.conversation_id.is_empty())
                .then(|| actor.conversation_id.clone()),
            seat_id: None,
            agent_label: None,
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Tool,
            tool: ACTION.to_owned(),
            summary: summary(&proposal.account, &proposal.items),
            cwd: None,
            paths: Vec::new(),
            risk: None,
            action_digest: digest,
            payload_redacted: Some(json!({"account": proposal.account, "items": rows})),
            ttl_seconds: TTL_SECONDS,
        };
        let approval = self
            .chat
            .create_mail_organize_approval(question)
            .map_err(chat_error)?;
        Ok(json!({
            "ok": true,
            "status": "pending",
            "approval_id": approval.id,
            "count": proposal.items.len(),
            "expires_in": TTL_SECONDS,
            "notice": PENDING_NOTICE,
        }))
    }
}

pub(crate) fn dest_of(folder: MoveFolder) -> MoveDest {
    match folder {
        MoveFolder::Junk => MoveDest::Junk,
        MoveFolder::Deleted => MoveDest::Deleted,
        MoveFolder::Inbox => MoveDest::Inbox,
    }
}

/// What the operator reads on the card: how many messages go where, and the account.
fn summary(account: &str, items: &[ProposedMove]) -> String {
    let count = |folder: MoveFolder| items.iter().filter(|item| item.folder == folder).count();
    let parts: Vec<String> = [
        (MoveFolder::Junk, "a no deseado"),
        (MoveFolder::Deleted, "a eliminados"),
        (MoveFolder::Inbox, "a la bandeja"),
    ]
    .into_iter()
    .filter(|(folder, _)| count(*folder) > 0)
    .map(|(folder, label)| format!("{} {label}", count(folder)))
    .collect();
    let noun = if items.len() == 1 {
        "correo"
    } else {
        "correos"
    };
    format!(
        "Mover {} {noun} ({}) — {account}",
        items.len(),
        parts.join(", ")
    )
}

/// What the chat refused, in mail terms. A digest another question holds is a stale
/// batch; a digest the operator refused is a batch that must change to be asked again.
pub(crate) fn chat_error(error: ChatError) -> MailError {
    match error {
        ChatError::ApprovalDigestReused => MailError::MoveStale,
        ChatError::ApprovalDigestRefused => MailError::MoveRefused,
        ChatError::NoSuchApproval => MailError::InvalidInput(Redacted::new("unknown approval")),
        other => MailError::ApprovalUnavailable(Redacted::new(other.to_string())),
    }
}
