//! The rules of an approval: how a row becomes a record, how the audit is
//! written, when an approval dies and what the host can resolve about the
//! request. The lifecycle itself lives in [`super::approvals`].

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::ChatError;
use crate::model::ChatFrame;
use crate::model_approval::{
    Approval, ApprovalAudit, ApprovalKind, ApprovalStatus, NewApproval, Risk,
};

use super::approvals::APPROVAL_SELECT;
use super::{append_event, conversation_exists};

/// The columns of one approval row, before the enums are resolved.
pub(super) struct ApprovalColumns {
    pub id: String,
    pub conversation_id: Option<String>,
    pub seat_id: Option<String>,
    pub agent_label: Option<String>,
    pub run_id: Option<String>,
    pub pane_id: Option<String>,
    pub kind: String,
    pub tool: String,
    pub summary: String,
    pub cwd: Option<String>,
    pub paths_json: String,
    pub risk: String,
    pub action_digest: String,
    pub status: String,
    pub scope: String,
    pub expires_at: String,
    pub decided_by: Option<String>,
    pub decided_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub payload_redacted: Option<String>,
    pub level: Option<String>,
    pub escalated_from: Option<String>,
    pub decider_kind: Option<String>,
    pub decider_id: Option<String>,
    pub reason: Option<String>,
    pub risk_score: Option<f64>,
    pub scorer_version: Option<String>,
}

pub(super) fn approval_row(row: &Row<'_>) -> rusqlite::Result<ApprovalColumns> {
    Ok(ApprovalColumns {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        seat_id: row.get(2)?,
        agent_label: row.get(3)?,
        run_id: row.get(4)?,
        pane_id: row.get(5)?,
        kind: row.get(6)?,
        tool: row.get(7)?,
        summary: row.get(8)?,
        cwd: row.get(9)?,
        paths_json: row.get(10)?,
        risk: row.get(11)?,
        action_digest: row.get(12)?,
        status: row.get(13)?,
        scope: row.get(14)?,
        expires_at: row.get(15)?,
        decided_by: row.get(16)?,
        decided_at: row.get(17)?,
        created_at: row.get(18)?,
        updated_at: row.get(19)?,
        payload_redacted: row.get(20)?,
        level: row.get(21)?,
        escalated_from: row.get(22)?,
        decider_kind: row.get(23)?,
        decider_id: row.get(24)?,
        reason: row.get(25)?,
        risk_score: row.get(26)?,
        scorer_version: row.get(27)?,
    })
}

pub(super) fn approval_from_columns(columns: ApprovalColumns) -> Result<Approval, ChatError> {
    let paths: Vec<String> = serde_json::from_str(&columns.paths_json)
        .map_err(|_| ChatError::Inconsistent("the approval paths are not a list"))?;
    let payload = match columns.payload_redacted {
        Some(raw) => Some(
            serde_json::from_str::<serde_json::Value>(&raw)
                .map_err(|_| ChatError::Inconsistent("the approval payload is not JSON"))?,
        ),
        None => None,
    };
    Ok(Approval {
        id: columns.id,
        conversation_id: columns.conversation_id,
        seat_id: columns.seat_id,
        agent_label: columns.agent_label,
        run_id: columns.run_id,
        pane_id: columns.pane_id,
        kind: ApprovalKind::from_db(&columns.kind)
            .ok_or(ChatError::Inconsistent("unknown approval kind"))?,
        tool: columns.tool,
        summary: columns.summary,
        cwd: columns.cwd,
        paths,
        risk: Risk::from_db(&columns.risk)
            .ok_or(ChatError::Inconsistent("unknown approval risk"))?,
        action_digest: columns.action_digest,
        status: ApprovalStatus::from_db(&columns.status)
            .ok_or(ChatError::Inconsistent("unknown approval status"))?,
        scope: columns.scope,
        expires_at: columns.expires_at,
        decided_by: columns.decided_by,
        decided_at: columns.decided_at,
        created_at: columns.created_at,
        updated_at: columns.updated_at,
        payload_redacted: payload,
        level: columns.level,
        escalated_from: columns.escalated_from,
        decider_kind: columns.decider_kind,
        decider_id: columns.decider_id,
        reason: columns.reason,
        risk_score: columns.risk_score,
        scorer_version: columns.scorer_version,
        audit: Vec::new(),
    })
}

pub(super) fn approval_by_id(
    connection: &Connection,
    approval_id: &str,
) -> Result<Approval, ChatError> {
    let columns = connection
        .query_row(
            &format!("{APPROVAL_SELECT} WHERE id = ?1"),
            [approval_id],
            approval_row,
        )
        .optional()?
        .ok_or(ChatError::NoSuchApproval)?;
    approval_from_columns(columns)
}

pub(super) fn audit_of(
    connection: &Connection,
    approval_id: &str,
) -> Result<Vec<ApprovalAudit>, ChatError> {
    let mut statement = connection.prepare(
        "SELECT device_id, action_digest, decision, scope, decided_at
           FROM approval_audit WHERE approval_id = ?1 ORDER BY id ASC",
    )?;
    let rows = statement.query_map([approval_id], |row| {
        Ok(ApprovalAudit {
            device_id: row.get(0)?,
            action_digest: row.get(1)?,
            decision: row.get(2)?,
            scope: row.get(3)?,
            decided_at: row.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Who made a transition, as the audit trail names it: `policy`, `agent`,
/// `device`, `scorer` or `system` (the host's own expiry). `id` is the actor
/// itself, never a version: the scorer's model and prompt revision go in
/// `scorer_version`, which only a scorer row sets.
pub(super) struct Decider<'a> {
    pub kind: &'a str,
    pub id: &'a str,
    pub level: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub scorer_version: Option<&'a str>,
}

pub(super) fn write_audit(
    connection: &Connection,
    approval_id: &str,
    action_digest: &str,
    decider: &Decider<'_>,
    decision: &str,
    decided_at: &str,
) -> Result<(), ChatError> {
    connection.execute(
        "INSERT INTO approval_audit
            (approval_id, action_digest, device_id, decision, scope, decided_at,
             level, decider_kind, decider_id, reason, scorer_version)
         VALUES (?1, ?2, ?3, ?4, 'once', ?5, ?6, ?7, ?3, ?8, ?9)",
        params![
            approval_id,
            action_digest,
            decider.id,
            decision,
            decided_at,
            decider.level,
            decider.kind,
            decider.reason,
            decider.scorer_version
        ],
    )?;
    Ok(())
}

/// The `chat.approval` frame of one transition. `change` says which one:
/// `created`, `escalated`, `decided`, `consumed` or `expired`. The body carries
/// the level chain, so a client shows where the approval sits without asking.
pub(super) fn frame_of(
    connection: &Connection,
    approval_id: &str,
    change: &str,
) -> Result<ChatFrame, ChatError> {
    let approval = approval_by_id(connection, approval_id)?;
    append_event(
        connection,
        "chat.approval",
        approval.conversation_id.as_deref().unwrap_or_default(),
        serde_json::json!({
            "event": "chat.approval",
            "conversation_id": approval.conversation_id,
            "approval_id": approval.id,
            "agent_id": approval.seat_id,
            "pane_id": approval.pane_id,
            "change": change,
            "status": approval.status.as_str(),
            "level": approval.level,
            "escalated_from": approval.escalated_from,
            "decider_kind": approval.decider_kind,
            "decider_id": approval.decider_id,
            "risk_score": approval.risk_score,
            "scorer_version": approval.scorer_version,
            "kind": approval.kind.as_str(),
            "summary": approval.summary,
            "expires_at": approval.expires_at,
        }),
    )
}

/// What the host can resolve about the request: the conversation it belongs
/// to, the delegation that asked and the seat that will run it.
#[derive(Default, PartialEq)]
pub(super) struct ApprovalContext {
    pub conversation_id: Option<String>,
    pub delegation_id: Option<String>,
    pub seat_id: Option<String>,
}

pub(super) fn resolve_context(
    connection: &Connection,
    new: &NewApproval,
) -> Result<ApprovalContext, ChatError> {
    if let Some(conversation_id) = &new.conversation_id {
        if !conversation_exists(connection, conversation_id)? {
            return Err(ChatError::NoSuchConversation);
        }
        return Ok(ApprovalContext {
            conversation_id: Some(conversation_id.clone()),
            ..ApprovalContext::default()
        });
    }
    if let Some(delegation) = delegation_of_run(connection, &new.run_id)? {
        return Ok(ApprovalContext {
            conversation_id: Some(delegation.0),
            delegation_id: Some(delegation.1),
            seat_id: Some(delegation.2),
        });
    }
    if let Some(seat_id) = &new.seat_id {
        if let Some(delegation) = active_delegation_of(connection, "to_agent_id", seat_id)? {
            return Ok(ApprovalContext {
                conversation_id: Some(delegation.0),
                delegation_id: Some(delegation.1),
                seat_id: Some(seat_id.clone()),
            });
        }
    }
    if let Some(pane_id) = &new.pane_id {
        if let Some(delegation) = active_delegation_of(connection, "pane_id", pane_id)? {
            return Ok(ApprovalContext {
                conversation_id: Some(delegation.0),
                delegation_id: Some(delegation.1),
                seat_id: delegation.2,
            });
        }
    }
    Ok(ApprovalContext::default())
}

/// `(conversation_id, delegation_id, seat_id)` of the delegation a run owns.
fn delegation_of_run(
    connection: &Connection,
    run_id: &Option<String>,
) -> Result<Option<(String, String, String)>, ChatError> {
    let Some(run_id) = run_id else {
        return Ok(None);
    };
    Ok(connection
        .query_row(
            "SELECT conversation_id, id, to_agent_id FROM delegation WHERE run_id = ?1",
            [run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?)
}

/// `(conversation_id, delegation_id, seat_id)` of the active delegation of a
/// seat or of a pane. The column is a fixed literal, never a user input.
fn active_delegation_of(
    connection: &Connection,
    column: &str,
    value: &str,
) -> Result<Option<(String, String, Option<String>)>, ChatError> {
    let sql = format!(
        "SELECT conversation_id, id, to_agent_id FROM delegation
          WHERE {column} = ?1 AND state IN ('queued', 'running', 'waiting')
          ORDER BY created_at DESC LIMIT 1"
    );
    Ok(connection
        .query_row(&sql, [value], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .optional()?)
}

/// The approval that already holds an action digest: the digest is the identity
/// of the action, so a second request for it meets this row.
pub(super) struct DigestHolder {
    pub id: String,
    pub status: String,
    pub seat_id: Option<String>,
}

pub(super) fn digest_holder(
    connection: &Connection,
    digest: &str,
) -> Result<Option<DigestHolder>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT id, status, seat_id FROM approval WHERE action_digest = ?1",
            [digest],
            |row| {
                Ok(DigestHolder {
                    id: row.get(0)?,
                    status: row.get(1)?,
                    seat_id: row.get(2)?,
                })
            },
        )
        .optional()?)
}

/// The project a seat belongs to, `None` for a seat without project.
pub(super) fn project_of_seat(
    connection: &Connection,
    seat_id: &str,
) -> Result<Option<String>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT project_id FROM agent_seat WHERE id = ?1",
            [seat_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}

pub(super) fn pending_in_conversation(
    connection: &Connection,
    conversation_id: Option<&str>,
) -> Result<usize, ChatError> {
    let count: i64 = match conversation_id {
        Some(conversation_id) => connection.query_row(
            "SELECT COUNT(*) FROM approval
              WHERE status = 'pending' AND conversation_id = ?1",
            [conversation_id],
            |row| row.get(0),
        )?,
        None => connection.query_row(
            "SELECT COUNT(*) FROM approval WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )?,
    };
    Ok(count.max(0) as usize)
}
