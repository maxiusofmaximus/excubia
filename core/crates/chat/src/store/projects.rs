//! Projects and their seats. A project is created under the operator's
//! `workspaces_root` (the path is resolved and canonicalized by `hn-core-team`,
//! which owns the filesystem); this module owns the durable records and the
//! limits: one lead per project and at most eight visible seats.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::ChatError;
use crate::id;
use crate::model::{ProjectSummary, SeatSummary};
use crate::model_team::{NewProject, NewSeat, ProjectRecord, Seat, UAC_UNCHECKED};
use crate::schema;

use super::{company_id, root_label};

pub const MAX_SEATS_PER_PROJECT: usize = 8;
pub const DEFAULT_LEAD_NAME: &str = "Ernesto";

impl super::ChatStore {
    pub fn create_project(&self, new: NewProject) -> Result<ProjectSummary, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let company = company_id(&transaction)?;
        let root_path = new.root_path.to_string_lossy().to_string();
        if project_at(&transaction, &root_path)?.is_some() {
            return Err(ChatError::ProjectExists);
        }
        let now = schema::now(&transaction)?;
        let project_id = id::new_id("prj");
        transaction.execute(
            "INSERT INTO project (id, company_id, name, root_path, lead_agent_id, status, created_at)
             VALUES (?1, ?2, ?3, ?4, NULL, 'active', ?5)",
            params![project_id, company, new.name, root_path, now],
        )?;
        let lead = attach_lead(&transaction, &project_id, &now)?;
        transaction.execute(
            "UPDATE project SET lead_agent_id = ?1 WHERE id = ?2",
            params![lead.agent_id, project_id],
        )?;
        transaction.commit()?;
        Ok(ProjectSummary {
            id: project_id,
            name: new.name,
            root_label: root_label(&root_path),
            status: "active".to_string(),
            lead: Some(lead),
        })
    }

    pub fn project(&self, project_id: &str) -> Result<ProjectRecord, ChatError> {
        let connection = self.lock();
        project_by_id(&connection, project_id)
    }

    pub fn project_of_agent(&self, agent_id: &str) -> Result<Option<String>, ChatError> {
        let connection = self.lock();
        Ok(connection
            .query_row(
                "SELECT project_id FROM agent_seat WHERE id = ?1",
                [agent_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }

    /// The project a conversation belongs to. Delegation scope is checked
    /// against it, not against a client-supplied id.
    pub fn conversation_project(&self, conversation_id: &str) -> Result<Option<String>, ChatError> {
        let connection = self.lock();
        Ok(connection
            .query_row(
                "SELECT project_id FROM conversation WHERE id = ?1",
                [conversation_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }

    pub fn seat(&self, agent_id: &str) -> Result<Seat, ChatError> {
        let connection = self.lock();
        seat_by_id(&connection, agent_id)
    }

    /// Create a seat record. The caller starts its pane and stores the id with
    /// [`Self::set_seat_pane`]; a failure there leaves a visible draft seat
    /// instead of losing the record.
    pub fn create_seat(&self, new: NewSeat) -> Result<Seat, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let company = company_id(&transaction)?;
        if !matches!(new.role_id.as_str(), "lead" | "specialist") {
            return Err(ChatError::ScopeDenied);
        }
        project_by_id(&transaction, &new.project_id)?;
        let seats: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM agent_seat WHERE project_id = ?1 AND status != 'removed'",
            [&new.project_id],
            |row| row.get(0),
        )?;
        if seats as usize >= MAX_SEATS_PER_PROJECT {
            return Err(ChatError::SeatLimit);
        }
        if new.role_id == "lead" && project_lead(&transaction, &new.project_id)?.is_some() {
            return Err(ChatError::LeadExists(new.project_id));
        }
        let now = schema::now(&transaction)?;
        let agent_id = id::new_id("ag");
        transaction.execute(
            "INSERT INTO agent_seat
                (id, company_id, project_id, role_id, name, avatar, kind, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'unset', 'draft', ?6)",
            params![
                agent_id,
                company,
                new.project_id,
                new.role_id,
                new.name,
                now
            ],
        )?;
        transaction.commit()?;
        Ok(Seat {
            agent_id,
            name: new.name,
            role_id: new.role_id,
            project_id: Some(new.project_id),
            pane_id: None,
            status: "draft".to_string(),
            uac_state: UAC_UNCHECKED.to_string(),
        })
    }

    /// Attach a conversation to a project. A conversation belongs to one
    /// project: the delegation rules check the tree against it, so the host
    /// links the conversation when Alfred creates the project.
    pub fn set_conversation_project(
        &self,
        conversation_id: &str,
        project_id: &str,
    ) -> Result<(), ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        project_by_id(&transaction, project_id)?;
        let current: Option<String> = transaction
            .query_row(
                "SELECT project_id FROM conversation WHERE id = ?1",
                [conversation_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        match current.as_deref() {
            Some(existing) if existing != project_id => {
                return Err(ChatError::ScopeDenied);
            }
            _ => {}
        }
        transaction.execute(
            "UPDATE conversation SET project_id = ?1 WHERE id = ?2",
            params![project_id, conversation_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn set_seat_pane(&self, agent_id: &str, pane_id: Option<&str>) -> Result<(), ChatError> {
        let connection = self.lock();
        connection.execute(
            "UPDATE agent_seat SET pane_id = ?1 WHERE id = ?2",
            params![pane_id, agent_id],
        )?;
        Ok(())
    }

    pub fn project_lead(&self, project_id: &str) -> Result<Option<Seat>, ChatError> {
        let connection = self.lock();
        project_lead(&connection, project_id)
    }

    /// The specialist a lead may address: same project, role `specialist`.
    pub fn specialist_in(&self, agent_id: &str, project_id: &str) -> Result<Seat, ChatError> {
        let connection = self.lock();
        let seat = seat_by_id(&connection, agent_id)?;
        if seat.role_id != "specialist" || seat.project_id.as_deref() != Some(project_id) {
            return Err(ChatError::ScopeDenied);
        }
        Ok(seat)
    }
}

fn attach_lead(
    connection: &Connection,
    project_id: &str,
    now: &str,
) -> Result<SeatSummary, ChatError> {
    let free: Option<SeatSummary> = connection
        .query_row(
            "SELECT id, name, status, uac_state, pane_id FROM agent_seat
              WHERE role_id = 'lead' AND project_id IS NULL
              ORDER BY created_at ASC, id ASC LIMIT 1",
            [],
            |row| {
                Ok(SeatSummary {
                    agent_id: row.get(0)?,
                    name: row.get(1)?,
                    status: row.get(2)?,
                    uac_state: row.get(3)?,
                    pane_id: row.get(4)?,
                })
            },
        )
        .optional()?;
    if let Some(mut lead) = free {
        connection.execute(
            "UPDATE agent_seat SET project_id = ?1 WHERE id = ?2",
            params![project_id, lead.agent_id],
        )?;
        lead.status = "draft".to_string();
        return Ok(lead);
    }
    let company = company_id(connection)?;
    let agent_id = id::new_id("ag");
    connection.execute(
        "INSERT INTO agent_seat
            (id, company_id, project_id, role_id, name, avatar, kind, status, created_at)
         VALUES (?1, ?2, ?3, 'lead', ?4, 'ernesto', 'unset', 'draft', ?5)",
        params![agent_id, company, project_id, DEFAULT_LEAD_NAME, now],
    )?;
    Ok(SeatSummary {
        agent_id,
        name: DEFAULT_LEAD_NAME.to_string(),
        status: "draft".to_string(),
        uac_state: UAC_UNCHECKED.to_string(),
        pane_id: None,
    })
}

fn project_lead(connection: &Connection, project_id: &str) -> Result<Option<Seat>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT id, name, role_id, project_id, pane_id, status, uac_state FROM agent_seat
              WHERE project_id = ?1 AND role_id = 'lead' LIMIT 1",
            [project_id],
            seat_row,
        )
        .optional()?)
}

pub(super) fn seat_by_id(connection: &Connection, agent_id: &str) -> Result<Seat, ChatError> {
    connection
        .query_row(
            "SELECT id, name, role_id, project_id, pane_id, status, uac_state
               FROM agent_seat WHERE id = ?1",
            [agent_id],
            seat_row,
        )
        .optional()?
        .ok_or_else(|| ChatError::NoSuchSeat(agent_id.to_string()))
}

fn seat_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Seat> {
    Ok(Seat {
        agent_id: row.get(0)?,
        name: row.get(1)?,
        role_id: row.get(2)?,
        project_id: row.get(3)?,
        pane_id: row.get(4)?,
        status: row.get(5)?,
        uac_state: row.get(6)?,
    })
}

fn project_by_id(connection: &Connection, project_id: &str) -> Result<ProjectRecord, ChatError> {
    connection
        .query_row(
            "SELECT id, name, root_path, status, lead_agent_id FROM project WHERE id = ?1",
            [project_id],
            |row| {
                Ok(ProjectRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    root_path: row.get(2)?,
                    status: row.get(3)?,
                    lead_agent_id: row.get(4)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| ChatError::UnknownProject(project_id.to_string()))
}

fn project_at(connection: &Connection, root_path: &str) -> Result<Option<String>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT id FROM project WHERE root_path = ?1",
            [root_path],
            |row| row.get(0),
        )
        .optional()?)
}
