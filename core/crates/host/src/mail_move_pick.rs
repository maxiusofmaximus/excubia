//! Which approved batch `mail.move_apply` applies when the turn names none (F2b-3). An
//! approval id does not survive from one turn to the next: a turn starts from the chat
//! text, not from earlier tool results. So the host finds the batch itself: the batch of
//! the account that this conversation asked, approved, neither applied nor expired. One
//! is applied; none and several are answered, and nothing moves.

use hn_core_chat::{Approval, ApprovalFilter, ApprovalStatus, SeatActor};
use hn_core_mail::MailError;

use crate::mail_move::chat_error;
use crate::mail_move_apply::belongs_to;
use crate::mail_tools::MailTools;

/// How many approval ids an ambiguous answer lists.
const LISTED_IDS: usize = 5;

/// How many batches one listing reads. A conversation holds a few open batches at most.
const LISTED_LIMIT: usize = 200;

/// What the host makes of the batches of an account when the turn names none.
pub(crate) enum Pick {
    One(String),
    Nothing { pending: usize },
    Several(Vec<String>),
}

impl MailTools {
    /// The one approved batch of `account` that `actor`'s conversation asked, if any.
    pub(crate) fn chosen_approval(
        &self,
        actor: &SeatActor,
        account: &str,
    ) -> Result<Pick, MailError> {
        let now = self.chat.now_text();
        let mut ready: Vec<String> = self
            .batches_of(actor, account, ApprovalStatus::Approved)?
            .into_iter()
            .filter(|approval| approval.expires_at > now)
            .map(|approval| approval.id)
            .collect();
        match ready.len() {
            0 => Ok(Pick::Nothing {
                pending: self
                    .batches_of(actor, account, ApprovalStatus::Pending)?
                    .len(),
            }),
            1 => Ok(Pick::One(ready.remove(0))),
            _ => {
                ready.truncate(LISTED_IDS);
                Ok(Pick::Several(ready))
            }
        }
    }

    /// The batches of `account` in `status` that `actor`'s conversation asked, newest first.
    /// A turn outside a conversation asked none.
    fn batches_of(
        &self,
        actor: &SeatActor,
        account: &str,
        status: ApprovalStatus,
    ) -> Result<Vec<Approval>, MailError> {
        if actor.conversation_id.is_empty() {
            return Ok(Vec::new());
        }
        let filter = ApprovalFilter {
            status: Some(status),
            conversation_id: Some(actor.conversation_id.clone()),
            limit: LISTED_LIMIT,
        };
        let approvals = self.chat.approvals(&filter).map_err(chat_error)?;
        Ok(approvals
            .into_iter()
            .filter(|approval| belongs_to(approval, account))
            .collect())
    }
}

/// The field and the message of an ambiguous apply: the ids to name. Every other error
/// has a fixed message, so it is `None`.
pub(crate) fn ambiguous_reply(error: &MailError) -> Option<(Option<&'static str>, String)> {
    match error {
        MailError::AmbiguousApproval(ids) => Some((
            Some("approval_id"),
            format!("name one of these approval_id values: {}", ids.join(", ")),
        )),
        _ => None,
    }
}
