//! The approval mode (CORE9-D, RFC §5), kept in the database. A change is
//! audited with the device that made it and published as
//! `chat.approval_settings`. The pending approvals that the new mode sends to
//! the operator climb to it at once, so none is left waiting on an agent level
//! that no longer decides for it.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::json;

use crate::error::ChatError;
use crate::model::ChatFrame;
use crate::model_mode::{ApprovalMode, ApprovalSettings, ModeChange, ProjectMode};

use super::append_event;
use super::approval_rules::{frame_of, project_of_seat, write_audit, Decider};

/// The scope row of the global mode. A project id never equals it.
const GLOBAL: &str = "global";

/// The mode that applies to a project: its own row, else the global one.
pub(super) fn effective(
    connection: &Connection,
    project_id: Option<&str>,
) -> Result<ApprovalMode, ChatError> {
    if let Some(project_id) = project_id {
        if let Some(mode) = stored(connection, project_id)? {
            return Ok(mode);
        }
    }
    Ok(stored(connection, GLOBAL)?.unwrap_or_default())
}

fn stored(connection: &Connection, scope: &str) -> Result<Option<ApprovalMode>, ChatError> {
    let raw: Option<String> = connection
        .query_row(
            "SELECT mode FROM approval_mode WHERE scope = ?1",
            [scope],
            |row| row.get(0),
        )
        .optional()?;
    Ok(raw.as_deref().and_then(ApprovalMode::from_label))
}

fn settings(connection: &Connection) -> Result<ApprovalSettings, ChatError> {
    let global = stored(connection, GLOBAL)?.unwrap_or_default();
    let mut statement = connection
        .prepare("SELECT scope, mode FROM approval_mode WHERE scope != ?1 ORDER BY scope")?;
    let rows = statement.query_map([GLOBAL], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut projects = Vec::new();
    for row in rows {
        let (project_id, mode) = row?;
        if let Some(mode) = ApprovalMode::from_label(&mode) {
            projects.push(ProjectMode { project_id, mode });
        }
    }
    Ok(ApprovalSettings { global, projects })
}

fn project_exists(connection: &Connection, project_id: &str) -> Result<bool, ChatError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM project WHERE id = ?1",
        [project_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn applied(connection: &Connection, client_id: &str) -> Result<bool, ChatError> {
    let found: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM approval_settings_audit WHERE client_id = ?1",
            [client_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

/// The pending approvals that wait on an agent level and that the mode now
/// sends to the operator. Each one climbs to `user` and keeps the level it left.
fn climb_to_operator(
    transaction: &Transaction<'_>,
    device_id: &str,
    now: &str,
) -> Result<Vec<ChatFrame>, ChatError> {
    let waiting: Vec<(String, Option<String>, String, String)> = {
        let mut statement = transaction.prepare(
            "SELECT id, seat_id, level, action_digest FROM approval
              WHERE status = 'pending' AND level IN ('lead', 'cto')",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut frames = Vec::new();
    for (approval_id, seat_id, from, digest) in waiting {
        let project_id = match seat_id.as_deref() {
            Some(seat) => project_of_seat(transaction, seat)?,
            None => None,
        };
        if effective(transaction, project_id.as_deref())? != ApprovalMode::AlwaysUser {
            continue;
        }
        transaction.execute(
            "UPDATE approval
                SET level = 'user', escalated_from = ?2, reason = 'mode_changed',
                    level_expires_at = NULL, updated_at = ?3
              WHERE id = ?1",
            params![approval_id, from, now],
        )?;
        let decider = Decider {
            kind: "user",
            id: device_id,
            level: Some("user"),
            reason: Some("mode_changed"),
            scorer_version: None,
        };
        write_audit(
            transaction,
            &approval_id,
            &digest,
            &decider,
            "escalated",
            now,
        )?;
        frames.push(frame_of(transaction, &approval_id, "escalated")?);
    }
    Ok(frames)
}

impl super::ChatStore {
    /// The modes the operator set: the global one and the projects overriding it.
    pub fn approval_settings(&self) -> Result<ApprovalSettings, ChatError> {
        let connection = self.lock();
        settings(&connection)
    }

    /// Set the mode of the global scope (`project_id` `None`) or of one project.
    /// `device_id` is the signed device that asks: it is the decider of the
    /// audit. `client_id` is only the idempotency key: a `client_id` that was
    /// already applied changes nothing and answers the current settings. An
    /// accepted change is audited and published, and the approvals that it sends
    /// to the operator climb there at once.
    pub fn set_approval_mode(
        &self,
        client_id: &str,
        device_id: &str,
        project_id: Option<&str>,
        mode: ApprovalMode,
    ) -> Result<ModeChange, ChatError> {
        self.expire_now()?;
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        if applied(&transaction, client_id)? {
            return Ok(ModeChange {
                settings: settings(&transaction)?,
                replayed: true,
                escalated: 0,
            });
        }
        if let Some(project_id) = project_id {
            if !project_exists(&transaction, project_id)? {
                return Err(ChatError::UnknownProject(project_id.to_string()));
            }
        }
        let scope = project_id.unwrap_or(GLOBAL);
        let now = self.now_text();
        let from = effective(&transaction, project_id)?;
        transaction.execute(
            "INSERT INTO approval_mode (scope, mode, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(scope) DO UPDATE SET mode = excluded.mode, updated_at = excluded.updated_at",
            params![scope, mode.as_str(), now],
        )?;
        transaction.execute(
            "INSERT INTO approval_settings_audit
                (client_id, scope, from_mode, to_mode, decider_kind, decider_id,
                 level, reason, decided_at)
             VALUES (?1, ?2, ?3, ?4, 'user', ?5, 'user', 'mode_changed', ?6)",
            params![
                client_id,
                scope,
                from.as_str(),
                mode.as_str(),
                device_id,
                now
            ],
        )?;
        let mut frames = climb_to_operator(&transaction, device_id, &now)?;
        let escalated = frames.len() as u64;
        frames.push(append_event(
            &transaction,
            "chat.approval_settings",
            "",
            json!({
                "event": "chat.approval_settings",
                "scope": scope,
                "mode": mode.as_str(),
                "previous": from.as_str(),
                "escalated": escalated,
            }),
        )?);
        let settings = settings(&transaction)?;
        transaction.commit()?;
        drop(connection);
        for frame in frames {
            self.publish(frame);
        }
        Ok(ModeChange {
            settings,
            replayed: false,
            escalated,
        })
    }
}
