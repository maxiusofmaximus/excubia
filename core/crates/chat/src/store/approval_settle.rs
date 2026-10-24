//! The clock-driven moves of an approval (CORE9-C): a level that runs out of
//! its 40 s escalates, the TTL expires it, and a restart closes what waits.
//!
//! The climb is monotone: `policy` → `lead` → `cto` → `user`, one hop at a
//! time, never back. A level nobody can decide (no live lead, no live CTO) is
//! skipped, and any hop that would leave less than 15 s of TTL goes straight
//! to the operator. Nothing here approves or denies: a timeout only moves.

use rusqlite::{params, Connection, OptionalExtension};

use crate::clock::{iso, parse_iso};
use crate::error::ChatError;
use crate::model::ChatFrame;
use crate::model_approval::{Approval, ApprovalStatus};

use super::approval_level::ApprovalLevel;
use super::approval_policy::Placement;
use super::approval_rules::{approval_by_id, frame_of, project_of_seat, write_audit, Decider};
use super::next_seq;

/// How long an agent level has to decide before its request climbs.
pub(super) const LEVEL_MS: i64 = 40_000;

/// The least TTL that can still reach an agent level. Below it the request
/// goes to the operator, so no agent is asked for a decision it cannot make.
const ESCALATE_MIN_MS: i64 = 15_000;

/// The reason of a hop that skipped a level nobody can decide.
const DECIDER_UNAVAILABLE: &str = "decider_unavailable";

/// The reason of a level that ran out of time.
const TIMEOUT: &str = "timeout";

/// Where an approval goes when it leaves its level at `at_ms`.
pub(super) struct Hop {
    pub level: ApprovalLevel,
    pub reason: &'static str,
}

/// Whether a level has someone to decide. The lead of the project and the
/// CTO count while their seat is not stopped; a stopped seat has no panel.
fn available(
    connection: &Connection,
    level: ApprovalLevel,
    project_id: Option<&str>,
) -> Result<bool, ChatError> {
    let seats: i64 = match level {
        ApprovalLevel::Lead => match project_id {
            None => return Ok(false),
            Some(project_id) => connection.query_row(
                "SELECT COUNT(*) FROM agent_seat
                  WHERE role_id = 'lead' AND project_id = ?1 AND status != 'stopped'",
                [project_id],
                |row| row.get(0),
            )?,
        },
        ApprovalLevel::Cto => connection.query_row(
            "SELECT COUNT(*) FROM agent_seat WHERE role_id = 'alfred' AND status != 'stopped'",
            [],
            |row| row.get(0),
        )?,
        _ => 1,
    };
    Ok(seats > 0)
}

/// The hop an approval takes from `from` at `now_ms`. `reason` names why the
/// approval left its level; it is replaced by `decider_unavailable` when a
/// level was skipped on the way.
pub(super) fn hop(
    connection: &Connection,
    from: ApprovalLevel,
    project_id: Option<&str>,
    now_ms: i64,
    expires_ms: i64,
    reason: &'static str,
) -> Result<Hop, ChatError> {
    if expires_ms - now_ms < ESCALATE_MIN_MS {
        return Ok(Hop {
            level: ApprovalLevel::User,
            reason,
        });
    }
    let mut skipped = false;
    let mut candidate = from.next();
    while let Some(level) = candidate {
        if level == ApprovalLevel::User || available(connection, level, project_id)? {
            let reason = if skipped { DECIDER_UNAVAILABLE } else { reason };
            return Ok(Hop { level, reason });
        }
        skipped = true;
        candidate = level.next();
    }
    Ok(Hop {
        level: ApprovalLevel::User,
        reason,
    })
}

/// Route a new approval that the policy sent to an agent level: the placement
/// climbs past any level nobody can decide, and it gets its 40 s deadline.
pub(super) fn route(
    connection: &Connection,
    placement: &mut Placement,
    project_id: Option<&str>,
    has_conversation: bool,
    now_ms: i64,
    expires_ms: i64,
) -> Result<(), ChatError> {
    if placement.status == ApprovalStatus::Pending && !has_conversation {
        // No conversation means no agent that can answer it: only the operator
        // sees it, so it waits at `user` where the inbox counts it.
        placement.level = ApprovalLevel::User;
        placement.reason = Some(DECIDER_UNAVAILABLE);
    } else if placement.status == ApprovalStatus::Pending && placement.level == ApprovalLevel::Lead
    {
        let reason = placement.reason.unwrap_or("no_rule");
        let from = placement.escalated_from.unwrap_or(ApprovalLevel::Policy);
        let climbed = hop(connection, from, project_id, now_ms, expires_ms, reason)?;
        placement.level = climbed.level;
        placement.reason = Some(climbed.reason);
    }
    placement.level_expires_at = match placement.level {
        ApprovalLevel::Lead | ApprovalLevel::Cto if placement.status == ApprovalStatus::Pending => {
            Some(iso(now_ms + LEVEL_MS))
        }
        _ => None,
    };
    Ok(())
}

/// The approvals whose deadline passed, moved to where they belong. Each one
/// is walked up to its next future instant, so a late read gives the same
/// chain as a prompt one.
pub(super) fn settle_due(
    connection: &Connection,
    now_ms: i64,
) -> Result<Vec<ChatFrame>, ChatError> {
    let now = iso(now_ms);
    let due: Vec<String> = {
        let mut statement = connection.prepare(
            "SELECT id FROM approval
              WHERE status = 'pending'
                AND (expires_at <= ?1 OR level_expires_at <= ?1)",
        )?;
        let rows = statement.query_map([&now], |row| row.get(0))?;
        rows.collect::<Result<Vec<String>, _>>()?
    };
    let mut frames = Vec::new();
    for approval_id in due {
        frames.extend(settle_one(connection, &approval_id, now_ms)?);
    }
    Ok(frames)
}

fn settle_one(
    connection: &Connection,
    approval_id: &str,
    now_ms: i64,
) -> Result<Vec<ChatFrame>, ChatError> {
    let mut frames = Vec::new();
    loop {
        let Some((expires, level_deadline)) = deadlines(connection, approval_id)? else {
            return Ok(frames);
        };
        match level_deadline {
            Some(at) if at < expires && at <= now_ms => {
                frames.extend(escalate(connection, approval_id, at, expires)?);
            }
            _ if expires <= now_ms => {
                frames.extend(expire(connection, approval_id, expires)?);
                return Ok(frames);
            }
            _ => return Ok(frames),
        }
    }
}

/// The TTL and the level deadline of a pending approval, in milliseconds.
fn deadlines(
    connection: &Connection,
    approval_id: &str,
) -> Result<Option<(i64, Option<i64>)>, ChatError> {
    let row: Option<(String, Option<String>)> = connection
        .query_row(
            "SELECT expires_at, level_expires_at FROM approval
              WHERE id = ?1 AND status = 'pending'",
            [approval_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((expires, level_deadline)) = row else {
        return Ok(None);
    };
    let expires = parse_iso(&expires).ok_or(ChatError::Inconsistent("approval deadline"))?;
    let level_deadline = level_deadline.as_deref().and_then(parse_iso);
    Ok(Some((expires, level_deadline)))
}

/// The earliest instant at which a pending approval changes by itself.
pub(super) fn next_deadline_ms(
    connection: &Connection,
    approval_id: &str,
) -> Result<Option<i64>, ChatError> {
    Ok(deadlines(connection, approval_id)?
        .map(|(expires, level_deadline)| level_deadline.map_or(expires, |at| at.min(expires))))
}

/// A level ran out at `at_ms`: the request climbs, and the new level gets its
/// own 40 s only when it is an agent. Audited as the host's decision.
fn escalate(
    connection: &Connection,
    approval_id: &str,
    at_ms: i64,
    expires_ms: i64,
) -> Result<Vec<ChatFrame>, ChatError> {
    let approval = approval_by_id(connection, approval_id)?;
    let from = match approval.level.as_deref() {
        None => ApprovalLevel::Policy,
        Some(name) => {
            ApprovalLevel::from_column(name).ok_or(ChatError::Inconsistent("approval level"))?
        }
    };
    let project_id = match approval.seat_id.as_deref() {
        Some(seat_id) => project_of_seat(connection, seat_id)?,
        None => None,
    };
    let climbed = hop(
        connection,
        from,
        project_id.as_deref(),
        at_ms,
        expires_ms,
        TIMEOUT,
    )?;
    let level_deadline = matches!(climbed.level, ApprovalLevel::Lead | ApprovalLevel::Cto)
        .then(|| iso(at_ms + LEVEL_MS));
    let at = iso(at_ms);
    connection.execute(
        "UPDATE approval
            SET level = ?1, escalated_from = ?2, reason = ?3, level_expires_at = ?4, updated_at = ?5
          WHERE id = ?6 AND status = 'pending'",
        params![
            climbed.level.as_str(),
            from.as_str(),
            climbed.reason,
            level_deadline,
            at,
            approval_id
        ],
    )?;
    write_audit(
        connection,
        approval_id,
        &approval.action_digest,
        &Decider {
            kind: "system",
            id: "host",
            level: Some(climbed.level.as_str()),
            reason: Some(climbed.reason),
            scorer_version: None,
        },
        "escalated",
        &at,
    )?;
    Ok(vec![frame_of(connection, approval_id, "escalated")?])
}

/// The TTL ran out, or the host restarted: the approval is `expired`, decided
/// by the host, and its line goes to the timeline.
fn expire(
    connection: &Connection,
    approval_id: &str,
    at_ms: i64,
) -> Result<Vec<ChatFrame>, ChatError> {
    let approval = approval_by_id(connection, approval_id)?;
    let at = iso(at_ms);
    connection.execute(
        "UPDATE approval
            SET status = 'expired', decided_at = ?1, updated_at = ?1, level_expires_at = NULL,
                decider_kind = 'system', decider_id = 'host'
          WHERE id = ?2 AND status = 'pending'",
        params![at, approval_id],
    )?;
    write_audit(
        connection,
        approval_id,
        &approval.action_digest,
        &Decider {
            kind: "system",
            id: "host",
            level: None,
            reason: None,
            scorer_version: None,
        },
        "expired",
        &at,
    )?;
    let expired = approval_by_id(connection, approval_id)?;
    record_timeline_line(connection, &expired)?;
    Ok(vec![frame_of(connection, approval_id, "expired")?])
}

/// The host restarted: no process will deliver a decision that was pending,
/// so every pending approval expires now, as the host's own decision.
pub(super) fn expire_at_start(connection: &Connection, now_ms: i64) -> Result<(), ChatError> {
    let pending: Vec<String> = {
        let mut statement =
            connection.prepare("SELECT id FROM approval WHERE status = 'pending'")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<Result<Vec<String>, _>>()?
    };
    for approval_id in pending {
        expire(connection, &approval_id, now_ms)?;
    }
    Ok(())
}

/// The timeline line of an approval of a conversation. It is born `pending`
/// with the request and keeps its `seq` while it moves: each decision (the
/// operator's, the policy's, an agent's or the host's expiry) rewrites the same
/// line, so the timeline never shows two lines for one approval.
pub(super) fn record_timeline_line(
    connection: &Connection,
    approval: &Approval,
) -> Result<(), ChatError> {
    let Some(conversation_id) = approval.conversation_id.as_deref() else {
        return Ok(());
    };
    let seq = next_seq(connection, conversation_id)?;
    let summary = timeline_summary(connection, approval)?;
    connection.execute(
        "INSERT INTO timeline_approval
            (approval_id, conversation_id, seq, status, level, summary,
             decider_kind, decider_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(approval_id) DO UPDATE SET
            status = excluded.status,
            level = excluded.level,
            summary = excluded.summary,
            decider_kind = excluded.decider_kind,
            decider_id = excluded.decider_id",
        params![
            approval.id,
            conversation_id,
            seq,
            approval.status.as_str(),
            approval.level,
            summary,
            approval.decider_kind.as_deref().unwrap_or("system"),
            approval.decider_id,
            approval
                .decided_at
                .as_deref()
                .unwrap_or(&approval.updated_at),
        ],
    )?;
    Ok(())
}

fn timeline_summary(connection: &Connection, approval: &Approval) -> Result<String, ChatError> {
    if approval.status == ApprovalStatus::Pending {
        return Ok(format!("Pendiente: {}", approval.summary));
    }
    let who = match approval.decider_kind.as_deref() {
        Some("user") => "Operador".to_string(),
        Some("policy") => "Política".to_string(),
        Some("agent") => {
            let agent_id = approval.decider_id.clone().unwrap_or_default();
            connection
                .query_row(
                    "SELECT name FROM agent_seat WHERE id = ?1",
                    [&agent_id],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or(agent_id)
        }
        _ => "Host".to_string(),
    };
    let verb = match approval.status {
        ApprovalStatus::Approved => "aprobó",
        ApprovalStatus::Denied => "denegó",
        ApprovalStatus::Expired => "expiró",
        _ => "resolvió",
    };
    Ok(format!("{who} {verb}: {}", approval.summary))
}
