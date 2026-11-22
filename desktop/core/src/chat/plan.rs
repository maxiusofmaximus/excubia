//! «Plan propuesto»: the plans an agent proposes before it acts, over REST. Listing a plan
//! is `READ_STATE`; approving, rejecting or asking for changes is `APPROVE`, which the host
//! answers with `403` to a device without it. A plan id goes into a path, so it is checked
//! here before it leaves the device.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::error::{ChatFailure, ChatResult};
use super::validate_prefixed_id;
use crate::client::HostClient;

/// The codes the host words for a plan refusal. Any other code stays a host refusal.
const REFUSAL_CODES: [&str; 4] = [
    "no_such_plan",
    "plan_decided",
    "plan_not_approved",
    "plan_expired",
];
const NOTE_MAX_CHARS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Proposed,
    Approved,
    Rejected,
    ChangesRequested,
}

/// What a step touches. A step with `nothing` only reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanTouch {
    Files,
    Network,
    Mail,
    Tools,
    Nothing,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PlanStep {
    pub title: String,
    pub touches: Vec<PlanTouch>,
}

/// A plan as the host summarises it. `summary` is the host's own sentence, which says who proposes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Plan {
    pub id: String,
    pub status: PlanStatus,
    pub steps: Vec<PlanStep>,
    pub summary: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub delegation_id: Option<String>,
    #[serde(default)]
    pub decided_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub decided_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlanList {
    pub plans: Vec<Plan>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlanDecided {
    pub plan: Plan,
}

/// The host's refusal of a plan route, by its code.
pub(super) fn plan_refusal(body: &[u8]) -> Option<ChatFailure> {
    let code = super::error::detail_code(body)?;
    REFUSAL_CODES
        .contains(&code.as_str())
        .then_some(ChatFailure::PlanRefused { code })
}

/// `/v1/plans/{id}{action}`, with the id checked first.
fn plan_path(id: &str, action: &str) -> ChatResult<String> {
    validate_prefixed_id(id, "plan_", "plan")?;
    Ok(format!("/v1/plans/{id}{action}"))
}

/// The note the operator wrote for a change request: trimmed, present, and at most 300 characters.
fn checked_note(note: &str) -> ChatResult<String> {
    let text = note.trim();
    if text.is_empty() {
        return Err(ChatFailure::Invalid {
            message: "Escribe qué cambiarías del plan.".to_string(),
            field: Some("note".to_string()),
        });
    }
    if text.chars().count() > NOTE_MAX_CHARS {
        return Err(ChatFailure::Invalid {
            message: format!("La nota admite {NOTE_MAX_CHARS} caracteres como máximo."),
            field: Some("note".to_string()),
        });
    }
    Ok(text.to_string())
}

impl HostClient {
    /// Every plan the device can read, in the order the host sends them.
    pub async fn plans(&self) -> ChatResult<Vec<Plan>> {
        let list: PlanList = self.chat_get("/v1/plans", &[]).await?;
        Ok(list.plans)
    }

    pub async fn plan_approve(&self, id: &str) -> ChatResult<Plan> {
        self.plan_decide(id, "/approve", &json!({})).await
    }

    pub async fn plan_reject(&self, id: &str) -> ChatResult<Plan> {
        self.plan_decide(id, "/reject", &json!({})).await
    }

    pub async fn plan_request_changes(&self, id: &str, note: &str) -> ChatResult<Plan> {
        let note = checked_note(note)?;
        self.plan_decide(id, "/request_changes", &json!({ "note": note }))
            .await
    }

    async fn plan_decide(&self, id: &str, action: &str, body: &Value) -> ChatResult<Plan> {
        let path = plan_path(id, action)?;
        let decided: PlanDecided = self
            .chat_send_json(Method::POST, &path, &[], body, true)
            .await?;
        Ok(decided.plan)
    }
}

#[cfg(test)]
mod tests {
    use super::super::error::status_failure;
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn the_plans_read_with_their_steps_and_touches_as_the_host_sends_them() {
        let body = r#"{"ok":true,"plans":[{"id":"plan_01HZ","status":"proposed",
            "steps":[{"title":"Leer el repo","touches":["files"]},{"title":"Enviar","touches":["mail","network"]}],
            "summary":"Plan de 2 pasos · lo propone Ernesto · toca: ficheros, correo e internet",
            "delegation_id":null,"created_at":"2026-11-05T10:00:00.000Z","updated_at":"2026-11-05T10:00:00.000Z"}]}"#;
        let list: PlanList = serde_json::from_str(body).expect("plans");
        let plan = &list.plans[0];
        assert_eq!(plan.status, PlanStatus::Proposed);
        assert_eq!(
            plan.steps[1].touches,
            vec![PlanTouch::Mail, PlanTouch::Network]
        );
        assert_eq!(plan.note, None);
        assert_eq!(plan.decided_at, None);
    }

    #[test]
    fn every_status_and_touch_the_contract_names_is_read() {
        for (text, expected) in [
            ("proposed", PlanStatus::Proposed),
            ("approved", PlanStatus::Approved),
            ("rejected", PlanStatus::Rejected),
            ("changes_requested", PlanStatus::ChangesRequested),
        ] {
            let parsed: PlanStatus = serde_json::from_str(&format!("\"{text}\"")).expect("status");
            assert_eq!(parsed, expected);
        }
        for (text, expected) in [
            ("files", PlanTouch::Files),
            ("network", PlanTouch::Network),
            ("mail", PlanTouch::Mail),
            ("tools", PlanTouch::Tools),
            ("nothing", PlanTouch::Nothing),
        ] {
            let parsed: PlanTouch = serde_json::from_str(&format!("\"{text}\"")).expect("touch");
            assert_eq!(parsed, expected);
        }
    }

    #[test]
    fn a_status_the_contract_does_not_name_is_a_read_error() {
        assert!(serde_json::from_str::<PlanStatus>("\"sleeping\"").is_err());
    }

    #[test]
    fn a_plan_id_must_be_plain_before_it_reaches_a_path() {
        assert_eq!(
            plan_path("plan_01HZ", "/approve").expect("plain id"),
            "/v1/plans/plan_01HZ/approve"
        );
        assert!(plan_path("../plan_x", "/approve").is_err());
        assert!(plan_path("plan_a/b", "/reject").is_err());
        assert!(plan_path("cnv_01HZ", "/reject").is_err());
        assert!(plan_path("plan_", "/reject").is_err());
    }

    #[test]
    fn a_plan_refusal_keeps_its_code_and_other_codes_stay_host_refusals() {
        assert_eq!(
            plan_refusal(br#"{"detail":{"code":"plan_decided"}}"#),
            Some(ChatFailure::PlanRefused {
                code: "plan_decided".to_string()
            })
        );
        assert_eq!(
            plan_refusal(br#"{"detail":"no_such_plan"}"#),
            Some(ChatFailure::PlanRefused {
                code: "no_such_plan".to_string()
            })
        );
        assert_eq!(plan_refusal(br#"{"detail":{"code":"bad_pane"}}"#), None);
    }

    #[test]
    fn a_plan_conflict_reaches_the_screen_as_a_plan_refusal_not_a_host_one() {
        let body = br#"{"detail":{"code":"plan_decided"}}"#;
        let failure = status_failure(StatusCode::CONFLICT, body, None, true);
        assert_eq!(
            failure,
            ChatFailure::PlanRefused {
                code: "plan_decided".to_string()
            }
        );
        let missing = status_failure(
            StatusCode::NOT_FOUND,
            br#"{"detail":{"code":"no_such_plan"}}"#,
            None,
            false,
        );
        assert!(matches!(missing, ChatFailure::PlanRefused { .. }));
    }

    #[test]
    fn a_change_request_note_is_trimmed_and_limited_to_300_characters() {
        assert_eq!(checked_note("  Sin correo  ").expect("note"), "Sin correo");
        assert!(matches!(
            checked_note("   "),
            Err(ChatFailure::Invalid { .. })
        ));
        assert!(checked_note(&"é".repeat(NOTE_MAX_CHARS)).is_ok());
        assert!(matches!(
            checked_note(&"é".repeat(NOTE_MAX_CHARS + 1)),
            Err(ChatFailure::Invalid { field: Some(ref field), .. }) if field == "note"
        ));
    }
}
