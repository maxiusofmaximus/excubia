//! Undoing a move (F2A): the message goes back to the folder it left, under its new
//! id, when that folder is one [`MoveDest`] names. The undo is recorded as a move of
//! its own and the original is marked undone, so an action is undone only once.

use crate::actions::{ActionLog, MailActionRecord};
use crate::error::MailError;
use crate::graph::GraphClient;
use crate::mover::MoveDest;

const UNDO_TOOL: &str = "mail.undo";

impl GraphClient<'_> {
    /// Moves the message of `record_id` back to the folder it left and returns the
    /// record of that undo.
    pub async fn undo(
        &self,
        log: &ActionLog,
        record_id: &str,
    ) -> Result<MailActionRecord, MailError> {
        let record = log
            .find(self.account, record_id)?
            .ok_or(MailError::ActionNotFound)?;
        if record.undone {
            return Err(MailError::ActionUndone);
        }
        let dest = MoveDest::from_alias(&record.from_folder).ok_or(MailError::NotUndoable)?;
        let moved = self.move_message(&record.message_id_after, dest).await?;
        let undo = MailActionRecord::new(
            self.account,
            UNDO_TOOL,
            &record.message_id_after,
            &moved,
            &record.from_folder,
        );
        log.append(&undo)?;
        log.mark_undone(&record.id)?;
        Ok(undo)
    }
}
