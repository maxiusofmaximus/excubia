//! Applying a batch the moment the operator approves it (F2b-4). The decision answers
//! first; the batch then runs in the background through the path `mail.move_apply`
//! uses, and the conversation of the batch hears what happened. The report is written
//! by the host from the outcome alone: no subject and no sender ever reach the chat.

use std::sync::Arc;

use excubia_chat::{Approval, ApprovalStatus};
use excubia_mail::{MailError, MoveDest};
use serde_json::Value;

use crate::mail_move::ACTION;
use crate::mail_tools::MailTools;

impl MailTools {
    /// Runs `on_decided` in the background, so the decision does not wait for Microsoft.
    pub fn decided_in_background(self: &Arc<Self>, approval: Approval) {
        let tools = Arc::clone(self);
        tokio::spawn(async move { tools.on_decided(&approval).await });
    }

    /// Applies a mail batch the operator just approved and tells its conversation how it
    /// went. Another tool's approval, or one not approved, does nothing.
    pub async fn on_decided(&self, approval: &Approval) {
        if approval.tool != ACTION || !matches!(approval.status, ApprovalStatus::Approved) {
            return;
        }
        let Some(account) = approval
            .payload_redacted
            .as_ref()
            .and_then(|payload| payload["account"].as_str())
        else {
            return;
        };
        let outcome = self.apply_approval(account, &approval.id).await;
        // The apply that burned the batch first is the one that reports it.
        if matches!(outcome, Err(MailError::MoveAlreadyApplied)) {
            return;
        }
        let Some(conversation) = approval.conversation_id.as_deref() else {
            return;
        };
        if let Err(error) = self
            .chat
            .append_alfred_message(conversation, &report(&outcome))
        {
            eprintln!("host: could not write the report of a mail batch: {error}");
        }
    }
}

/// What the conversation is told: the stable code of a failed batch, or the count of
/// what moved, what the operator left out and what failed.
fn report(outcome: &Result<Value, MailError>) -> String {
    match outcome {
        Ok(reply) => {
            let results = reply["results"].as_array().map_or(&[][..], Vec::as_slice);
            summary(results)
        }
        Err(error) => format!("No pude mover el lote de correos: {}.", error.code()),
    }
}

fn summary(results: &[Value]) -> String {
    let moved: Vec<&Value> = results
        .iter()
        .filter(|item| item.get("new_message_id").is_some())
        .collect();
    let skipped = results
        .iter()
        .filter(|item| item.get("skipped").is_some())
        .count();
    let failed = results
        .iter()
        .filter(|item| item.get("error").is_some())
        .count();
    let mut sentences = vec![moved_sentence(&moved)];
    if skipped > 0 {
        sentences.push(format!("Dejaste fuera: {skipped}."));
    }
    if failed > 0 {
        sentences.push(format!("Fallos: {failed}."));
    }
    sentences.join(" ")
}

fn moved_sentence(moved: &[&Value]) -> String {
    if moved.is_empty() {
        return "No moví ningún correo.".to_owned();
    }
    let noun = if moved.len() == 1 {
        "correo"
    } else {
        "correos"
    };
    let mut destinations: Vec<&str> = Vec::new();
    for item in moved {
        let label = dest_label(item["dest"].as_str().unwrap_or_default());
        if !destinations.contains(&label) {
            destinations.push(label);
        }
    }
    format!("Moví {} {noun} a {}.", moved.len(), destinations.join(", "))
}

/// The name of a destination in the chat. Every alias the batch holds is one of the three.
fn dest_label(alias: &str) -> &'static str {
    match MoveDest::from_alias(alias) {
        Some(MoveDest::Junk) => "no deseado",
        Some(MoveDest::Deleted) => "eliminados",
        Some(MoveDest::Inbox) => "la bandeja",
        None => "otra carpeta",
    }
}
