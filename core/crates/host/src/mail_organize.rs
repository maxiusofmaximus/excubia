//! The organize tools on the host (F2b2): a batch is proposed for the operator's
//! decision (`mail_move`), the decided batch is applied (`mail_move_apply`), the recent
//! moves that can be undone are listed (`mail.undo_list`), and one moved message is put
//! back here. Undo needs no approval: it returns a message to the folder the operator
//! already had it in, and it is recorded in the log.

use excubia_core_chat::SeatActor;
use excubia_core_mail::{MailError, Redacted};
use excubia_core_seat::{MoveUndo, Organize, UndoList};
use serde_json::{json, Value};

use crate::mail_shadow::rfc3339;
use crate::mail_tools::MailTools;

impl MailTools {
    /// Runs one organize call. Failures carry the stable code, never a subject or an address.
    pub(crate) async fn organize(
        &self,
        actor: &SeatActor,
        call: Organize,
    ) -> Result<Value, MailError> {
        match call {
            Organize::Propose(proposal) => self.propose(actor, &proposal).await,
            Organize::Apply(apply) => self.apply(actor, &apply).await,
            Organize::Undo(undo) => self.undo_move(&undo).await,
            Organize::UndoList(list) => self.undo_list(&list),
        }
    }

    /// The newest moves of the account that are not undone yet: the action id, the
    /// folder they went to and the time. Read from the local log only, so no subject,
    /// sender or body is read or returned, and no network call is made.
    fn undo_list(&self, list: &UndoList) -> Result<Value, MailError> {
        if self.store.load(&list.account)?.is_none() {
            return Err(MailError::InvalidInput(Redacted::new(
                "no account with that label is signed in",
            )));
        }
        let actions: Vec<Value> = self
            .actions
            .list_recent(&list.account, usize::MAX)?
            .into_iter()
            .filter(|record| record.tool == "mail.move" && !record.undone)
            .take(list.n as usize)
            .map(|record| {
                json!({
                    "action_id": record.id,
                    "dest": record.to_folder,
                    "at": rfc3339(record.ts as i64),
                    "undone": false,
                })
            })
            .collect();
        Ok(json!({"ok": true, "account": list.account, "actions": actions}))
    }

    /// Moves the message of `action_id` back to the folder it left. The action log
    /// refuses an action that is unknown, undone already, or not from this account.
    async fn undo_move(&self, undo: &MoveUndo) -> Result<Value, MailError> {
        let access = self.session(&undo.account).await?;
        let client = self.client(&access, &undo.account);
        let restored = client.undo(&self.actions, &undo.action_id).await?;
        Ok(json!({
            "ok": true,
            "action_id": undo.action_id,
            "restored_message_id": restored.message_id_after,
        }))
    }
}
