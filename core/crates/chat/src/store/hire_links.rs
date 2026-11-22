//! The link between a hire and its seat (F1B). `agent_seat.hire_id` is the
//! link; a seat takes it once, and a dismissed hire takes none. What the seat is
//! doing comes from its own approvals and delegations, never from message text.

use rusqlite::{params, Connection, OptionalExtension, Row};

use super::projects::{insert_seat, seat_by_id};
use crate::error::ChatError;
use crate::model_approval::ApprovalKind;
use crate::model_team::{NewSeat, Seat};

/// One active hire with what the chat knows about its seat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HireLink {
    pub hire_id: String,
    pub name: String,
    pub catalog_role: String,
    pub hired_at: String,
    /// The pane of the linked seat, when the seat has one.
    pub seat_pane: Option<String>,
    /// The oldest approval the seat is still waiting on.
    pub approval: Option<PendingApproval>,
    /// When the seat's running delegation started.
    pub working_since: Option<String>,
    /// When the seat's latest delegation finished.
    pub finished_at: Option<String>,
    /// The oldest plan the seat proposed that the operator has not answered yet.
    pub plan: Option<PendingPlan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingApproval {
    pub kind: ApprovalKind,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingPlan {
    pub steps: u32,
    pub created_at: String,
}

impl super::ChatStore {
    /// One link per active hire, the oldest hire first. A hire without a seat
    /// has every seat fact empty.
    pub fn hire_links(&self) -> Result<Vec<HireLink>, ChatError> {
        let connection = self.lock();
        let mut statement = connection.prepare(
            "SELECT h.id, h.name, h.catalog_role, h.created_at, s.pane_id,
                    (SELECT a.kind FROM approval a
                      WHERE a.seat_id = s.id AND a.status = 'pending'
                      ORDER BY a.created_at ASC LIMIT 1),
                    (SELECT a.created_at FROM approval a
                      WHERE a.seat_id = s.id AND a.status = 'pending'
                      ORDER BY a.created_at ASC LIMIT 1),
                    (SELECT r.started_at FROM delegation d JOIN run r ON r.id = d.run_id
                      WHERE d.to_agent_id = s.id AND d.state IN ('running', 'waiting')
                      ORDER BY d.created_at DESC LIMIT 1),
                    (SELECT d.finished_at FROM delegation d
                      WHERE d.to_agent_id = s.id AND d.state = 'done'
                      ORDER BY d.finished_at DESC LIMIT 1),
                    (SELECT json_array_length(p.steps) FROM plan p
                      WHERE p.proposer_agent_id = s.id AND p.status = 'proposed'
                      ORDER BY p.created_at ASC LIMIT 1),
                    (SELECT p.created_at FROM plan p
                      WHERE p.proposer_agent_id = s.id AND p.status = 'proposed'
                      ORDER BY p.created_at ASC LIMIT 1)
               FROM agent_hire h
               LEFT JOIN agent_seat s ON s.hire_id = h.id
              WHERE h.status = 'hired'
              ORDER BY h.created_at ASC, h.id ASC",
        )?;
        let rows = statement
            .query_map([], link_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Links a seat to an active hire. `false` when the seat already has a hire,
    /// the hire already has a seat, or the hire is not active.
    pub fn link_seat_to_hire(&self, seat_id: &str, hire_id: &str) -> Result<bool, ChatError> {
        let connection = self.lock();
        let linked = connection.execute(
            "UPDATE agent_seat SET hire_id = ?2
              WHERE id = ?1 AND hire_id IS NULL
                AND EXISTS (SELECT 1 FROM agent_hire WHERE id = ?2 AND status = 'hired')
                AND NOT EXISTS (SELECT 1 FROM agent_seat WHERE hire_id = ?2)",
            params![seat_id, hire_id],
        )?;
        Ok(linked == 1)
    }

    /// The seat that works for an active hire in `project_id`: the hire's own
    /// seat, or a new one linked to it now. `None` when `target` is not an
    /// active hire, so a plain seat id goes on as before.
    pub fn seat_for_hire(&self, target: &str, project_id: &str) -> Result<Option<Seat>, ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let Some(name) = active_hire_name(&transaction, target)? else {
            return Ok(None);
        };
        if let Some(seat) = linked_seat(&transaction, target)? {
            return Ok(Some(seat));
        }
        let new = NewSeat {
            project_id: project_id.to_string(),
            name,
            role_id: "specialist".to_string(),
        };
        let seat = insert_seat(&transaction, new, Some(target))?;
        transaction.commit()?;
        Ok(Some(seat))
    }
}

/// Whether a seat works for an active hire. A recipe reaches only those seats.
pub(super) fn seat_is_hire(connection: &Connection, agent_id: &str) -> Result<bool, ChatError> {
    let linked: i64 = connection.query_row(
        "SELECT COUNT(*) FROM agent_seat s
           JOIN agent_hire h ON h.id = s.hire_id
          WHERE s.id = ?1 AND h.status = 'hired'",
        [agent_id],
        |row| row.get(0),
    )?;
    Ok(linked > 0)
}

fn active_hire_name(connection: &Connection, hire_id: &str) -> Result<Option<String>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT name FROM agent_hire WHERE id = ?1 AND status = 'hired'",
            [hire_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// The seat a hire has, unless it was removed.
fn linked_seat(connection: &Connection, hire_id: &str) -> Result<Option<Seat>, ChatError> {
    let seat_id: Option<String> = connection
        .query_row(
            "SELECT id FROM agent_seat WHERE hire_id = ?1 AND status != 'removed' LIMIT 1",
            [hire_id],
            |row| row.get(0),
        )
        .optional()?;
    seat_id.map(|id| seat_by_id(connection, &id)).transpose()
}

fn link_row(row: &Row<'_>) -> rusqlite::Result<HireLink> {
    let kind: Option<String> = row.get(5)?;
    let approval_since: Option<String> = row.get(6)?;
    let approval = match (kind, approval_since) {
        (Some(kind), Some(created_at)) => Some(PendingApproval {
            kind: ApprovalKind::from_db(&kind).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    "unknown approval kind".into(),
                )
            })?,
            created_at,
        }),
        _ => None,
    };
    let plan_steps: Option<i64> = row.get(9)?;
    let plan_since: Option<String> = row.get(10)?;
    let plan = match (plan_steps, plan_since) {
        (Some(steps), Some(created_at)) => Some(PendingPlan {
            steps: u32::try_from(steps).unwrap_or(0),
            created_at,
        }),
        _ => None,
    };
    Ok(HireLink {
        hire_id: row.get(0)?,
        name: row.get(1)?,
        catalog_role: row.get(2)?,
        hired_at: row.get(3)?,
        seat_pane: row.get(4)?,
        approval,
        working_since: row.get(7)?,
        finished_at: row.get(8)?,
        plan,
    })
}

#[cfg(test)]
mod tests {
    use crate::store::ChatStore;
    use crate::{NewHire, NewSeat};

    fn hire(store: &ChatStore, role: &str) -> String {
        store
            .hire(NewHire {
                catalog_role: role.to_string(),
                area: "programacion".to_string(),
                name: "Bob".to_string(),
                avatar: None,
                engine: None,
                profile: None,
                skills_allow: Vec::new(),
            })
            .expect("hire")
            .into_hire()
            .id
    }

    fn project(store: &ChatStore) -> String {
        store
            .create_project(crate::NewProject {
                name: "Demo".to_string(),
                root_path: "C:\\work\\demo".into(),
            })
            .expect("project")
            .id
    }

    fn seat(store: &ChatStore, project_id: &str) -> String {
        store
            .create_seat(NewSeat {
                project_id: project_id.to_string(),
                name: "Bob".to_string(),
                role_id: "specialist".to_string(),
            })
            .expect("seat")
            .agent_id
    }

    #[test]
    fn a_hire_without_a_seat_has_no_activity() {
        let store = ChatStore::in_memory().expect("store");
        let hire_id = hire(&store, "bob");

        let links = store.hire_links().expect("links");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].hire_id, hire_id);
        assert_eq!(links[0].catalog_role, "bob");
        assert_eq!(links[0].seat_pane, None);
        assert_eq!(links[0].approval, None);
        assert_eq!(links[0].working_since, None);
        assert_eq!(links[0].finished_at, None);
    }

    #[test]
    fn a_seat_takes_one_hire_and_a_hire_takes_one_seat() {
        let store = ChatStore::in_memory().expect("store");
        let hire_id = hire(&store, "bob");
        let project_id = project(&store);
        let first = seat(&store, &project_id);
        let second = seat(&store, &project_id);

        assert!(store.link_seat_to_hire(&first, &hire_id).expect("link"));
        assert!(!store.link_seat_to_hire(&second, &hire_id).expect("again"));
        assert!(!store
            .link_seat_to_hire(&second, "hire_missing")
            .expect("missing hire"));
        assert_eq!(store.hire_links().expect("links").len(), 1);
    }

    #[test]
    fn a_dismissed_hire_takes_no_seat_and_leaves_the_team() {
        let store = ChatStore::in_memory().expect("store");
        let hire_id = hire(&store, "bob");
        store.dismiss_hire(&hire_id).expect("dismiss");

        let project_id = project(&store);
        let seat_id = seat(&store, &project_id);
        assert!(!store.link_seat_to_hire(&seat_id, &hire_id).expect("link"));
        assert!(store.hire_links().expect("links").is_empty());
    }

    fn seats_of(store: &ChatStore, hire_id: &str) -> i64 {
        store
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM agent_seat WHERE hire_id = ?1",
                [hire_id],
                |row| row.get(0),
            )
            .expect("count")
    }

    #[test]
    fn the_first_delegation_gives_a_hire_one_seat_and_later_ones_reuse_it() {
        let store = ChatStore::in_memory().expect("store");
        let hire_id = hire(&store, "bob");
        let project_id = project(&store);

        let first = store
            .seat_for_hire(&hire_id, &project_id)
            .expect("first")
            .expect("a hire");
        let again = store
            .seat_for_hire(&hire_id, &project_id)
            .expect("again")
            .expect("a hire");

        assert_eq!(first.agent_id, again.agent_id);
        assert_eq!(first.name, "Bob");
        assert_eq!(first.role_id, "specialist");
        assert_eq!(first.project_id.as_deref(), Some(project_id.as_str()));
        assert_eq!(seats_of(&store, &hire_id), 1);
        assert_eq!(store.hire_links().expect("links")[0].seat_pane, None);
    }

    #[test]
    fn a_seat_linked_by_hand_is_the_one_a_hire_uses() {
        let store = ChatStore::in_memory().expect("store");
        let hire_id = hire(&store, "bob");
        let project_id = project(&store);
        let linked = seat(&store, &project_id);
        assert!(store.link_seat_to_hire(&linked, &hire_id).expect("link"));

        let found = store
            .seat_for_hire(&hire_id, &project_id)
            .expect("found")
            .expect("a hire");

        assert_eq!(found.agent_id, linked);
        assert_eq!(seats_of(&store, &hire_id), 1);
    }

    #[test]
    fn a_missing_or_dismissed_hire_is_not_a_hire_target() {
        let store = ChatStore::in_memory().expect("store");
        let project_id = project(&store);
        let dismissed = hire(&store, "bob");
        store.dismiss_hire(&dismissed).expect("dismiss");

        assert!(store
            .seat_for_hire("hire_missing", &project_id)
            .expect("missing")
            .is_none());
        assert!(store
            .seat_for_hire(&dismissed, &project_id)
            .expect("dismissed")
            .is_none());
        assert_eq!(seats_of(&store, &dismissed), 0);
    }

    #[test]
    fn dismissing_a_hire_removes_its_seat() {
        let store = ChatStore::in_memory().expect("store");
        let hire_id = hire(&store, "bob");
        let project_id = project(&store);
        let seat_id = store
            .seat_for_hire(&hire_id, &project_id)
            .expect("seat")
            .expect("a hire")
            .agent_id;

        store.dismiss_hire(&hire_id).expect("dismiss");

        assert_eq!(store.seat(&seat_id).expect("seat").status, "removed");
    }
}
