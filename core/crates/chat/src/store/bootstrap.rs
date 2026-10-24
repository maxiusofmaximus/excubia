//! The one-call home screen: company, projects, Alfred, unread counters and
//! the chat revision (`orchestrator-chat.md` §3).

use std::path::Path;

use crate::error::ChatError;
use crate::model::{Bootstrap, ChatSummary, Company, ProjectSummary, SeatSummary, Unread};

use super::delegation_rules::active_delegations;
use super::{company_id, revision, seat_summary};

impl super::ChatStore {
    pub fn bootstrap(&self) -> Result<Bootstrap, ChatError> {
        let connection = self.lock();
        let company = company(&connection)?;
        let alfred: SeatSummary =
            seat_summary(&connection, "alfred")?.ok_or(ChatError::NotInitialised)?;
        let projects = projects(&connection, &company.id)?;
        let unread = unread(&connection, &company.id)?;
        let revision = revision(&connection)?;
        let active_delegations = active_delegations(&connection)?;
        let pending_approvals = pending_approvals(&connection)?;
        let pending_user_approvals = pending_user_approvals(&connection)?;
        let chat_unread = unread.messages;
        Ok(Bootstrap {
            company,
            projects,
            alfred,
            unread,
            chat: ChatSummary {
                revision,
                unread: chat_unread,
                pending_approvals,
                pending_user_approvals,
                active_delegations,
            },
        })
    }

    /// The additive `chat` block of `/v1/state` and the opening SSE frame.
    pub fn summary(&self) -> Result<ChatSummary, ChatError> {
        let connection = self.lock();
        let company = company_id(&connection)?;
        let unread = unread(&connection, &company)?;
        Ok(ChatSummary {
            revision: revision(&connection)?,
            unread: unread.messages,
            pending_approvals: pending_approvals(&connection)?,
            pending_user_approvals: pending_user_approvals(&connection)?,
            active_delegations: active_delegations(&connection)?,
        })
    }
}

/// The approvals that wait for a decision (CORE8).
fn pending_approvals(connection: &rusqlite::Connection) -> Result<u64, ChatError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM approval WHERE status = 'pending'",
        [],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as u64)
}

/// The pending approvals that wait at the operator, not at an agent level.
fn pending_user_approvals(connection: &rusqlite::Connection) -> Result<u64, ChatError> {
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM approval WHERE status = 'pending' AND level = 'user'",
        [],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as u64)
}

fn company(connection: &rusqlite::Connection) -> Result<Company, ChatError> {
    let id = company_id(connection)?;
    let name = connection.query_row("SELECT name FROM company WHERE id = ?1", [&id], |row| {
        row.get(0)
    })?;
    Ok(Company { id, name })
}

fn projects(
    connection: &rusqlite::Connection,
    company_id: &str,
) -> Result<Vec<ProjectSummary>, ChatError> {
    let mut statement = connection.prepare(
        "SELECT p.id, p.name, p.root_path, p.status,
                s.id, s.name, s.status, s.uac_state, s.pane_id
           FROM project p
           LEFT JOIN agent_seat s ON s.id = p.lead_agent_id
          WHERE p.company_id = ?1 AND p.status != 'archived'
          ORDER BY p.created_at ASC, p.id ASC",
    )?;
    let rows = statement.query_map([company_id], |row| {
        let root_path: String = row.get(2)?;
        let lead_id: Option<String> = row.get(4)?;
        let lead = match lead_id {
            Some(agent_id) => Some(SeatSummary {
                agent_id,
                name: row.get(5)?,
                status: row.get(6)?,
                uac_state: row.get(7)?,
                pane_id: row.get(8)?,
            }),
            None => None,
        };
        Ok(ProjectSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            root_label: root_label(&root_path),
            status: row.get(3)?,
            lead,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn unread(connection: &rusqlite::Connection, company_id: &str) -> Result<Unread, ChatError> {
    let conversations: i64 = connection.query_row(
        "SELECT COUNT(*) FROM conversation c
          WHERE c.company_id = ?1 AND EXISTS (
                SELECT 1 FROM message m
                 WHERE m.conversation_id = c.id AND m.author_kind = 'agent'
                   AND m.seq > c.last_read_seq)",
        [company_id],
        |row| row.get(0),
    )?;
    let messages: i64 = connection.query_row(
        "SELECT COUNT(*) FROM message m JOIN conversation c ON c.id = m.conversation_id
          WHERE c.company_id = ?1 AND m.author_kind = 'agent' AND m.seq > c.last_read_seq",
        [company_id],
        |row| row.get(0),
    )?;
    Ok(Unread {
        conversations: conversations.max(0) as u64,
        messages: messages.max(0) as u64,
        approvals: pending_approvals(connection)?,
    })
}

/// The contract publishes `root_label`, never the host path.
pub(super) fn root_label(root_path: &str) -> String {
    Path::new(root_path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(root_path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_hide_the_parent_path() {
        assert_eq!(root_label(r"C:\work\demo"), "demo");
        assert_eq!(root_label("/home/me/demo/"), "demo");
        assert_eq!(root_label("demo"), "demo");
    }

    #[test]
    fn the_seeded_host_has_alfred_and_no_invented_project() {
        let store = super::super::ChatStore::in_memory().expect("store");
        let bootstrap = store.bootstrap().expect("bootstrap");
        assert_eq!(bootstrap.company.name, "Mi empresa");
        assert_eq!(bootstrap.alfred.name, "Alfred");
        assert!(bootstrap.projects.is_empty());
        assert_eq!(bootstrap.chat.revision, 0);
    }

    #[test]
    fn an_unread_agent_reply_is_counted() {
        let store = super::super::ChatStore::in_memory().expect("store");
        let conversation = store
            .create_conversation(crate::model::NewConversation {
                title: Some("Chat".to_string()),
                project_id: None,
            })
            .expect("conversation");
        let accepted = store
            .append_user_message(&conversation.id, "client-unread", "hola")
            .expect("message");
        store
            .complete_run_with_reply(&accepted.turn.id, "hola")
            .expect("reply");
        let bootstrap = store.bootstrap().expect("bootstrap");
        assert_eq!(bootstrap.unread.conversations, 1);
        assert_eq!(bootstrap.unread.messages, 1);
        store.mark_read(&conversation.id, 2).expect("read");
        assert_eq!(store.bootstrap().expect("bootstrap").unread.messages, 0);
    }
}
