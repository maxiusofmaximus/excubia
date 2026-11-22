//! «Copias del turno»: the restore points of an agent over REST. Listing a point and
//! what it would change is `READ_STATE`; putting the folder back is `APPROVE`, which the
//! host answers with `403` to a device without it. A point id goes into a path, so it is
//! checked here before it leaves the device.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::error::{detail_code, ChatFailure, ChatResult};
use crate::client::HostClient;

/// The codes the host words for a restore refusal. Any other code stays a host refusal.
const REFUSAL_CODES: [&str; 3] = [
    "restore_point_not_found",
    "restore_invalid",
    "restore_failed",
];
const POINT_ID_MAX: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreKind {
    Turn,
    BeforeUndo,
}

/// One saved state of an agent's folder, as the host summarises it: never a folder path.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RestorePoint {
    pub id: String,
    pub kind: RestoreKind,
    pub turn: String,
    pub created_ms: u64,
    pub files: u64,
    pub excluded: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RestorePoints {
    pub agent: String,
    pub points: Vec<RestorePoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExcludedReason {
    Name,
    TooLarge,
    Symlink,
    Special,
    UnsupportedName,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RestoreChange {
    pub path: String,
    pub status: ChangeStatus,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RestoreExcluded {
    pub path: String,
    pub reason: ExcludedReason,
}

/// What a restore of the point would change, and what the folder leaves out.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RestoreChanges {
    pub point: String,
    pub changes: Vec<RestoreChange>,
    pub excluded: Vec<RestoreExcluded>,
}

/// The answer to a restore. `undo_point` keeps the state from before it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RestoreDone {
    pub point: String,
    pub restored: u64,
    pub removed: u64,
    pub undo_point: String,
}

fn validate_point_id(id: &str) -> ChatResult<()> {
    let valid = !id.is_empty()
        && id.len() <= POINT_ID_MAX
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric());
    if valid {
        Ok(())
    } else {
        Err(ChatFailure::Invalid {
            message: "El identificador de la copia no es válido.".to_string(),
            field: None,
        })
    }
}

/// `/v1/restore/points/{id}{action}`, with the id checked first.
fn point_path(id: &str, action: &str) -> ChatResult<String> {
    validate_point_id(id)?;
    Ok(format!("/v1/restore/points/{id}{action}"))
}

/// The host's refusal of a restore route, by its code.
pub(super) fn restore_refusal(body: &[u8]) -> Option<ChatFailure> {
    let code = detail_code(body)?;
    REFUSAL_CODES
        .contains(&code.as_str())
        .then_some(ChatFailure::RestoreRefused { code })
}

impl HostClient {
    pub async fn restore_points(&self, agent: &str) -> ChatResult<RestorePoints> {
        self.chat_get("/v1/restore/points", &[("agent", agent.to_string())])
            .await
    }

    pub async fn restore_changes(&self, point_id: &str) -> ChatResult<RestoreChanges> {
        let path = point_path(point_id, "/files")?;
        self.chat_get(&path, &[]).await
    }

    pub async fn restore_point(&self, point_id: &str) -> ChatResult<RestoreDone> {
        let path = point_path(point_id, "/restore")?;
        self.chat_send_json(Method::POST, &path, &[], &json!({}), true)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_points_of_an_agent_read_as_the_host_sends_them() {
        let body = r#"{"agent":"w1:p2","points":[
            {"id":"01HZX0A1B2","kind":"before_undo","turn":"01HZX0A1B1","created_ms":1700000000000,"files":4,"excluded":1},
            {"id":"01HZX0A1B1","kind":"turn","turn":"t-7","created_ms":1699999000000,"files":0,"excluded":0}]}"#;
        let list: RestorePoints = serde_json::from_str(body).expect("points");
        assert_eq!(list.agent, "w1:p2");
        assert_eq!(list.points[0].kind, RestoreKind::BeforeUndo);
        assert_eq!(list.points[1].kind, RestoreKind::Turn);
        assert_eq!(list.points[0].files, 4);
    }

    #[test]
    fn a_change_and_an_exclusion_keep_their_status_and_reason() {
        let body = r#"{"point":"01HZX0A1B1",
            "changes":[{"path":"src/a.txt","status":"added"},{"path":"b.md","status":"deleted"}],
            "excluded":[{"path":"id_rsa","reason":"name"},{"path":"big.iso","reason":"too_large"}]}"#;
        let changes: RestoreChanges = serde_json::from_str(body).expect("changes");
        assert_eq!(changes.changes[0].status, ChangeStatus::Added);
        assert_eq!(changes.changes[1].status, ChangeStatus::Deleted);
        assert_eq!(changes.excluded[0].reason, ExcludedReason::Name);
        assert_eq!(changes.excluded[1].reason, ExcludedReason::TooLarge);
    }

    #[test]
    fn the_undo_answer_names_the_point_that_keeps_the_state_before() {
        let body = r#"{"point":"01HZX0A1B1","restored":3,"removed":1,"undo_point":"01HZX0A1B9"}"#;
        let done: RestoreDone = serde_json::from_str(body).expect("restored");
        assert_eq!(done.undo_point, "01HZX0A1B9");
        assert_eq!((done.restored, done.removed), (3, 1));
    }

    #[test]
    fn a_point_id_must_be_plain_before_it_reaches_a_path() {
        assert_eq!(
            point_path("01HZX0A1B1", "/files").expect("plain id"),
            "/v1/restore/points/01HZX0A1B1/files"
        );
        assert!(point_path("../../etc", "/files").is_err());
        assert!(point_path("a/b", "/restore").is_err());
        assert!(point_path("", "/restore").is_err());
        assert!(point_path(&"a".repeat(65), "/restore").is_err());
    }

    #[test]
    fn a_restore_refusal_keeps_its_code_and_other_codes_stay_host_refusals() {
        let body = br#"{"detail":{"code":"restore_invalid","message":"x"}}"#;
        assert_eq!(
            restore_refusal(body),
            Some(ChatFailure::RestoreRefused {
                code: "restore_invalid".to_string()
            })
        );
        assert_eq!(restore_refusal(br#"{"detail":{"code":"bad_pane"}}"#), None);
    }
}
