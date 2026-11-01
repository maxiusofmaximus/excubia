//! The organize tools on the host (F2b2): a batch is proposed for the operator's
//! decision (`mail_move`), the decided batch is applied (`mail_move_apply`), and one
//! moved message is put back here. Undo needs no approval: it returns a message to
//! the folder the operator already had it in, and it is recorded in the log.

use hn_core_chat::SeatActor;
use hn_core_mail::MailError;
use hn_core_seat::{MoveUndo, Organize};
use serde_json::{json, Value};

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
        }
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
