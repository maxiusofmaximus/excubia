//! The operator's exclusions from a batch of mail moves (F2C1). An approve may leave
//! out some messages: each one is marked `excluded` in the payload the operator
//! decides on, so the batch keeps its whole list and its digest does not move. Only
//! a pending batch of `mail.move` takes them, and at least one message must still move.

use rusqlite::{params, Transaction};
use serde_json::Value;

use crate::error::ChatError;
use crate::model_approval::{Approval, Decision};

const MOVE_TOOL: &str = "mail.move";

/// Marks the excluded messages of `approval` in the transaction that decides it. An
/// empty list changes nothing.
pub(super) fn mark(
    transaction: &Transaction<'_>,
    approval: &Approval,
    decision: Decision,
    exclude: &[String],
) -> Result<(), ChatError> {
    if exclude.is_empty() {
        return Ok(());
    }
    let payload = marked_payload(approval, decision, exclude)?;
    transaction.execute(
        "UPDATE approval SET payload_redacted = ?1 WHERE id = ?2",
        params![payload.to_string(), approval.id],
    )?;
    Ok(())
}

/// The batch's payload with the excluded items marked, once every rule has held.
fn marked_payload(
    approval: &Approval,
    decision: Decision,
    exclude: &[String],
) -> Result<Value, ChatError> {
    if decision != Decision::Approve {
        return Err(ChatError::InvalidExclusion(
            "only an approval takes exclusions",
        ));
    }
    if approval.tool != MOVE_TOOL {
        return Err(ChatError::InvalidExclusion(
            "only a batch of mail moves takes exclusions",
        ));
    }
    let mut payload = approval
        .payload_redacted
        .clone()
        .ok_or(ChatError::Inconsistent("a mail batch has no payload"))?;
    let items = payload["items"]
        .as_array_mut()
        .ok_or(ChatError::Inconsistent("a mail batch has no items"))?;
    let is_excluded = |item: &Value| {
        item["message_id"]
            .as_str()
            .is_some_and(|id| exclude.iter().any(|excluded| excluded == id))
    };
    for id in exclude {
        if !items
            .iter()
            .any(|item| item["message_id"].as_str() == Some(id.as_str()))
        {
            return Err(ChatError::NoSuchMessage(id.clone()));
        }
    }
    if items.iter().all(is_excluded) {
        return Err(ChatError::InvalidExclusion(
            "every message of the batch is excluded: deny the batch instead",
        ));
    }
    for item in items.iter_mut().filter(|item| is_excluded(item)) {
        item["excluded"] = Value::Bool(true);
    }
    Ok(payload)
}

/// How many messages of a decided batch the operator left out, for the timeline line.
pub(super) fn left_out(approval: &Approval) -> usize {
    let items = approval
        .payload_redacted
        .as_ref()
        .and_then(|p| p["items"].as_array());
    items.map_or(0, |items| {
        items.iter().filter(|item| item["excluded"] == true).count()
    })
}
