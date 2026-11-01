//! What the notes and the contacts share (F2, F3): the `agenda_audit` line of every
//! write, and the delete a user's approval burns. Alfred never deletes on its own word:
//! a delete question waits for the user, and only its approval reaches [`delete_object`].

use rusqlite::{params, Transaction};

use super::{calendar, contacts, notes};
use crate::error::ChatError;
use crate::model_approval::Approval;
use crate::model_calendar::CALENDAR_DELETE_TOOL;
use crate::model_contact::CONTACT_DELETE_TOOL;
use crate::model_note::NOTE_DELETE_TOOL;
use crate::seat::SeatActor;

/// Who makes a write of a note or a contact. Its kind is what the row and its audit
/// line keep: `alfred` for a seat, `device` for the app, `user` for the user who
/// approved a delete.
#[derive(Clone, Copy, Debug)]
pub(super) struct Writer<'a> {
    pub(super) kind: &'static str,
    pub(super) id: &'a str,
}

impl<'a> Writer<'a> {
    pub(super) fn seat(actor: &'a SeatActor) -> Self {
        Self {
            kind: "alfred",
            id: &actor.agent_id,
        }
    }

    pub(super) fn device(id: &'a str) -> Self {
        Self { kind: "device", id }
    }

    /// The events of an `.ics` file the app sent: the kind the event keeps as its origin.
    pub(super) fn import(id: &'a str) -> Self {
        Self { kind: "import", id }
    }

    pub(super) fn user(id: &'a str) -> Self {
        Self { kind: "user", id }
    }
}

/// Whether a row sorts after the cursor row in the newest-first order every list uses.
pub(super) fn older_than(updated_at: &str, id: &str, cursor: (&str, &str)) -> bool {
    (updated_at, id) < cursor
}

/// The payload key that names the object a delete question is about; `None` for any
/// other tool.
pub(super) fn delete_key(tool: &str) -> Option<&'static str> {
    match tool {
        NOTE_DELETE_TOOL => Some("note_id"),
        CONTACT_DELETE_TOOL => Some("contact_id"),
        CALENDAR_DELETE_TOOL => Some("event_id"),
        _ => None,
    }
}

/// Appends the audit line of one write. It names the object and its actor, never the
/// text of the object.
pub(super) fn write_audit(
    transaction: &Transaction<'_>,
    object_kind: &str,
    object_id: &str,
    action: &str,
    actor_kind: &str,
    actor_id: &str,
    at: &str,
) -> Result<(), ChatError> {
    transaction.execute(
        "INSERT INTO agenda_audit (object_kind, object_id, action, actor_kind, actor_id, at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![object_kind, object_id, action, actor_kind, actor_id, at],
    )?;
    Ok(())
}

/// Deletes the object an approved question names. It runs in the transaction that
/// consumes the approval, so an object already gone is no failure.
pub(super) fn delete_object(
    transaction: &Transaction<'_>,
    approval: &Approval,
    approver: &str,
    now: &str,
) -> Result<(), ChatError> {
    let Some(key) = delete_key(&approval.tool) else {
        return Ok(());
    };
    let object_id = approval
        .payload_redacted
        .as_ref()
        .and_then(|payload| payload[key].as_str());
    let Some(object_id) = object_id else {
        return Ok(());
    };
    match approval.tool.as_str() {
        NOTE_DELETE_TOOL => notes::delete_in(transaction, object_id, Writer::user(approver), now),
        CALENDAR_DELETE_TOOL => {
            calendar::delete_in(transaction, object_id, Writer::user(approver), now)
        }
        _ => contacts::delete_in(transaction, object_id, Writer::user(approver), now),
    }
}
