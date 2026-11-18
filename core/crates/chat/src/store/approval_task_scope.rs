//! Two answers beyond «once»: «always in this task» and «deny, and say why».
//!
//! «Always in this task» lets one seat repeat the same kind of action in the same
//! folder until its task ends: the next card it raises in that task is approved in
//! the operator's name as it is filed. It never reaches outside the folder the
//! operator approved, another seat, another task, or a mail action. A denial may carry
//! the operator's words, which the agent's hook passes on so the agent knows why.

use rusqlite::{params, OptionalExtension};

use crate::error::ChatError;
use crate::model_approval::{Approval, ApprovalStatus, Decision, MAIL_ACTIONS};

/// The longest reason a denial carries.
pub const MAX_DENIAL_REASON_CHARS: usize = 300;

/// A path written the same way on every platform: forward slashes, lower case, no
/// trailing slash. Only for comparing one path against a folder.
fn folded(path: &str) -> String {
    path.trim_start_matches(r"\\?\")
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

fn inside(folder: &str, path: &str) -> bool {
    let (folder, path) = (folded(folder), folded(path));
    path == folder || path.starts_with(&format!("{folder}/"))
}

impl super::ChatStore {
    /// Whether this approval may be answered «always in this task»: it belongs to a
    /// task, it is not a mail action, and everything it names is inside its folder.
    pub fn task_scope_allowed(&self, approval_id: &str) -> Result<bool, ChatError> {
        let connection = self.lock();
        let row: Option<(Option<String>, String, Option<String>, String)> = connection
            .query_row(
                "SELECT delegation_id, tool, cwd, paths_json FROM approval WHERE id = ?1",
                [approval_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((delegation, tool, cwd, paths)) = row else {
            return Err(ChatError::NoSuchApproval);
        };
        let (Some(_), Some(cwd)) = (delegation, cwd.filter(|cwd| !cwd.is_empty())) else {
            return Ok(false);
        };
        let paths: Vec<String> = serde_json::from_str(&paths).unwrap_or_default();
        Ok(!MAIL_ACTIONS.contains(&tool.as_str()) && paths.iter().all(|path| inside(&cwd, path)))
    }

    /// Marks an approved card as «always in this task». False when it is not allowed or
    /// the card was not approved.
    pub fn grant_task_scope(&self, approval_id: &str) -> Result<bool, ChatError> {
        if !self.task_scope_allowed(approval_id)? {
            return Ok(false);
        }
        let connection = self.lock();
        let changed = connection.execute(
            "UPDATE approval SET scope = 'task'
              WHERE id = ?1 AND delegation_id IS NOT NULL
                AND status IN ('approved', 'consumed')",
            [approval_id],
        )?;
        Ok(changed > 0)
    }

    /// Keeps the operator's words on a card he is about to deny.
    pub fn note_denial_reason(&self, approval_id: &str, reason: &str) -> Result<(), ChatError> {
        let reason: String = reason
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(MAX_DENIAL_REASON_CHARS)
            .collect();
        let reason = reason.trim();
        if reason.is_empty() {
            return Ok(());
        }
        let connection = self.lock();
        connection.execute(
            "UPDATE approval SET reason = ?1, operator_reason = ?1
              WHERE id = ?2 AND status = 'pending'",
            params![reason, approval_id],
        )?;
        Ok(())
    }

    /// A card just filed by a seat inside a task where the operator said «always»: it is
    /// approved in his name, with the same device, when it is the same kind of action of
    /// the same seat in the same folder.
    pub(super) fn apply_task_grant(&self, approval: Approval) -> Result<Approval, ChatError> {
        if approval.status != ApprovalStatus::Pending || !self.task_scope_allowed(&approval.id)? {
            return Ok(approval);
        }
        let granted_by: Option<String> = {
            let connection = self.lock();
            connection
                .query_row(
                    "SELECT g.decided_by FROM approval g JOIN approval n ON n.id = ?1
                      WHERE g.id <> n.id AND g.scope = 'task'
                        AND g.status IN ('approved', 'consumed')
                        AND g.delegation_id = n.delegation_id AND g.seat_id = n.seat_id
                        AND g.kind = n.kind AND g.tool = n.tool AND g.cwd = n.cwd
                        AND g.decided_by IS NOT NULL
                      LIMIT 1",
                    [&approval.id],
                    |row| row.get(0),
                )
                .optional()?
        };
        match granted_by {
            Some(device) => self.decide_approval(
                &approval.id,
                Decision::Approve,
                &format!("task-scope:{}", approval.id),
                &device,
            ),
            None => Ok(approval),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::inside;

    #[test]
    fn a_path_is_inside_its_folder_whatever_the_slashes_and_case() {
        assert!(inside(
            r"C:\Users\Max\bench-proj",
            "c:/users/max/bench-proj/notas.txt"
        ));
        assert!(inside(r"\\?\C:\proj", r"C:\proj\src\a.rs"));
        assert!(!inside(r"C:\proj", r"C:\proj-otro\a.rs"));
        assert!(!inside(r"C:\proj", r"C:\Windows\a.dll"));
    }
}
