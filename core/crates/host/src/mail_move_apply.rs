//! Applying a decided batch of moves (F2b2, F2b-3). The approval is read and its digest is
//! recomputed from the payload it was asked with. Only an approved batch with that
//! digest is burned, and only then do the messages move, one by one, in the order
//! proposed. A message that fails does not stop the rest. Each move that happens is
//! recorded in the action log under its new id. Without an id, the batch is the one
//! `mail_move_pick` finds for the conversation.

use hn_core_chat::{Approval, ApprovalStatus, ChatError, SeatActor};
use hn_core_mail::{
    batch_digest, FolderAliases, GraphClient, MailActionRecord, MailError, MoveDest, Redacted,
};
use hn_core_seat::MoveApply;
use serde_json::{json, Value};

use crate::mail_move::{chat_error, ACTION};
use crate::mail_move_pick::Pick;
use crate::mail_tools::MailTools;

impl MailTools {
    /// Applies the batch `apply` names, or the one approved batch of its account when it
    /// names none. Nothing approved answers `nothing_approved`; several answer
    /// `ambiguous_approval` with their ids. Neither moves a message.
    pub(crate) async fn apply(
        &self,
        actor: &SeatActor,
        apply: &MoveApply,
    ) -> Result<Value, MailError> {
        let approval_id = match &apply.approval_id {
            Some(id) => id.clone(),
            None => match self.chosen_approval(actor, &apply.account)? {
                Pick::One(id) => id,
                Pick::Nothing { pending } => {
                    return Ok(
                        json!({"ok": true, "status": "nothing_approved", "pending": pending}),
                    )
                }
                Pick::Several(ids) => return Err(MailError::AmbiguousApproval(ids)),
            },
        };
        self.apply_approval(&apply.account, &approval_id).await
    }

    /// The reply depends on the decision: a batch still open, refused or expired moves
    /// nothing. A batch already applied is refused, so it never runs twice.
    pub(crate) async fn apply_approval(
        &self,
        account: &str,
        approval_id: &str,
    ) -> Result<Value, MailError> {
        let approval = self.chat.approval(approval_id).map_err(chat_error)?;
        let batch = Batch::of(&approval, account)?;
        match approval.status {
            ApprovalStatus::Pending => return Ok(json!({"ok": true, "status": "pending"})),
            ApprovalStatus::Denied => return Ok(json!({"ok": true, "status": "denied"})),
            ApprovalStatus::Expired => return Ok(json!({"ok": true, "status": "expired"})),
            ApprovalStatus::Consumed => return Err(MailError::MoveAlreadyApplied),
            ApprovalStatus::Approved => {}
        }
        if batch.digest() != approval.action_digest {
            // The approval is spent for good: the owner must decide a new batch.
            let _ = self
                .chat
                .consume_for_host(&approval.id, &approval.action_digest);
            return Err(MailError::MoveStale);
        }
        let access = self.session(account).await?;
        let client = self.client(&access, account);
        let aliases = client.folder_aliases().await?;
        self.burn(&approval)?;
        let mut results = Vec::with_capacity(batch.entries.len());
        for entry in &batch.entries {
            results.push(
                self.move_entry(&client, &aliases, &approval, &batch, entry)
                    .await?,
            );
        }
        Ok(json!({"ok": true, "status": "applied", "results": results}))
    }

    /// Burns the approval before the first move, so two applies cannot both move.
    fn burn(&self, approval: &Approval) -> Result<(), MailError> {
        match self
            .chat
            .consume_for_host(&approval.id, &approval.action_digest)
        {
            Ok(_) => Ok(()),
            Err(ChatError::ApprovalAlreadyDecided) => Err(MailError::MoveAlreadyApplied),
            Err(error) => Err(chat_error(error)),
        }
    }

    /// One message of the batch. The owner may have left it out: then it is skipped and
    /// nothing is asked of Graph. A failed move is a result with its code and the batch
    /// goes on; a move that happened is logged, and a log that cannot be written fails the call.
    async fn move_entry(
        &self,
        client: &GraphClient<'_>,
        aliases: &FolderAliases,
        approval: &Approval,
        batch: &Batch,
        entry: &Entry,
    ) -> Result<Value, MailError> {
        if entry.excluded {
            return Ok(json!({
                "message_id": entry.message_id,
                "dest": entry.dest.alias(),
                "skipped": "owner",
            }));
        }
        match client
            .move_message_with(&entry.message_id, entry.dest, aliases)
            .await
        {
            Ok(moved) => {
                let mut record = MailActionRecord::new(
                    &batch.account,
                    "mail.move",
                    &entry.message_id,
                    &moved,
                    entry.dest.alias(),
                );
                record.verdict_level = Some(entry.level.clone());
                record.decider_kind = Some("owner".to_owned());
                record.approval_id = Some(approval.id.clone());
                self.actions.append(&record)?;
                Ok(json!({
                    "message_id": entry.message_id,
                    "dest": entry.dest.alias(),
                    "new_message_id": moved.id,
                    "action_id": record.id,
                }))
            }
            Err(error) => Ok(json!({
                "message_id": entry.message_id,
                "dest": entry.dest.alias(),
                "error": error.code(),
            })),
        }
    }
}

/// The batch as the approval was asked with it, read back from its payload.
struct Batch {
    account: String,
    entries: Vec<Entry>,
}

/// One message of a batch: where it goes, the level the host gave it when asked, and
/// whether the owner left it out when deciding.
struct Entry {
    message_id: String,
    dest: MoveDest,
    level: String,
    excluded: bool,
}

impl Batch {
    /// The batch of an approval for `account`. Another tool's approval, or one of
    /// another account, is not this batch.
    fn of(approval: &Approval, account: &str) -> Result<Self, MailError> {
        let payload = approval.payload_redacted.as_ref().ok_or_else(not_a_batch)?;
        if !belongs_to(approval, account) {
            return Err(not_a_batch());
        }
        let items = payload["items"].as_array().ok_or_else(not_a_batch)?;
        let entries = items.iter().map(Entry::of).collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            account: account.to_owned(),
            entries,
        })
    }

    /// The digest of the batch as the payload holds it now. It must equal the digest
    /// the approval binds, or the batch changed after it was asked.
    fn digest(&self) -> String {
        batch_digest(
            &self.account,
            self.entries
                .iter()
                .map(|entry| (entry.message_id.as_str(), entry.dest)),
        )
    }
}

impl Entry {
    fn of(item: &Value) -> Result<Self, MailError> {
        let dest = item["dest"]
            .as_str()
            .and_then(MoveDest::from_alias)
            .ok_or_else(not_a_batch)?;
        Ok(Self {
            message_id: text(item, "message_id")?,
            dest,
            level: text(item, "level")?,
            excluded: item["excluded"] == true,
        })
    }
}

/// Whether `approval` is a batch of moves of `account`. Another tool's approval, or one
/// of another account, is not.
pub(crate) fn belongs_to(approval: &Approval, account: &str) -> bool {
    approval.tool == ACTION
        && approval
            .payload_redacted
            .as_ref()
            .and_then(|payload| payload["account"].as_str())
            == Some(account)
}

fn text(item: &Value, field: &str) -> Result<String, MailError> {
    item[field]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(not_a_batch)
}

fn not_a_batch() -> MailError {
    MailError::InvalidInput(Redacted::new(
        "the approval is not a batch of moves for this account",
    ))
}
