//! «Automatizar»: the scheduled tasks of a hired professional, over REST. Listing and history are
//! `READ_STATE`; every write, pause, resume and run is `APPROVE`. The host decides what is due and
//! whether a run starts; the screen only asks and shows the answer. Ids go into a path or a body,
//! so they are checked here before they leave the device.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::error::{ChatFailure, ChatResult};
use super::{protocol, validate_prefixed_id};
use crate::client::HostClient;

/// The codes of a schedule refusal. A plan refusal and a recipe's secret keep their own types.
const REFUSAL_CODES: [&str; 5] = [
    "invalid_schedule",
    "schedule_limit",
    "schedule_done",
    "schedule_no_future_run",
    "no_such_schedule",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleStatus {
    Active,
    Paused,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Started,
    SkippedCap,
    SkippedOverlap,
    MissedOffline,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Schedule {
    pub id: String,
    pub title: String,
    pub conversation_id: String,
    pub role_id: String,
    pub body: Option<String>,
    pub recipe_id: Option<String>,
    pub rule: Option<String>,
    pub start_date: String,
    pub time_local: String,
    pub timezone: String,
    pub steps: u32,
    pub status: ScheduleStatus,
    pub plan_id: Option<String>,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One due instant of a schedule and what became of it. The history never carries the text.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ScheduleRun {
    pub id: String,
    pub schedule_id: String,
    pub due_at: String,
    pub started_at: Option<String>,
    pub outcome: RunOutcome,
    pub delegation_id: Option<String>,
}

/// What a create or an edit sends. Exactly one of `body` and `recipe_id` is set; `rule` is absent
/// for a one-time task. The host checks every field again.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ScheduleDraft {
    pub title: String,
    pub conversation_id: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe_id: Option<String>,
    pub start_date: String,
    pub time: String,
    pub timezone: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    pub steps: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
}

/// The answer to «Lanzar ahora»: the encargo the host opened, or the run it skipped (a brake or an
/// overlap). Only one of the two is set.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ScheduleLaunch {
    pub delegation_id: Option<String>,
    pub status: Option<String>,
    pub run: Option<ScheduleRun>,
}

#[derive(Debug, Deserialize)]
struct ScheduleList {
    schedules: Vec<Schedule>,
}

#[derive(Debug, Deserialize)]
struct ScheduleSaved {
    schedule: Schedule,
}

#[derive(Debug, Deserialize)]
struct RunList {
    runs: Vec<ScheduleRun>,
}

#[derive(Debug, Deserialize)]
struct ScheduleDeleted {
    id: String,
}

#[derive(Debug, Deserialize)]
struct RefusalBody {
    detail: RefusalDetail,
}

#[derive(Debug, Deserialize)]
struct RefusalDetail {
    code: String,
    #[serde(default)]
    message: Option<String>,
}

/// The host's refusal of a schedule route. For `invalid_schedule` the reason names the field
/// that is wrong (`rule`, `time`, `steps`…); the screen words it.
pub(super) fn schedule_refusal(body: &[u8]) -> Option<ChatFailure> {
    let detail = serde_json::from_slice::<RefusalBody>(body).ok()?.detail;
    if !REFUSAL_CODES.contains(&detail.code.as_str()) {
        return None;
    }
    let reason = if detail.code == "invalid_schedule" {
        detail.message
    } else {
        None
    };
    Some(ChatFailure::ScheduleRefused {
        code: detail.code,
        reason,
    })
}

impl ScheduleDraft {
    fn checked(&self) -> ChatResult<()> {
        validate_prefixed_id(&self.conversation_id, "cnv_", "conversación")?;
        if let Some(recipe) = &self.recipe_id {
            validate_prefixed_id(recipe, "rcp_", "receta")?;
        }
        if let Some(plan) = &self.plan_id {
            validate_prefixed_id(plan, "plan_", "plan")?;
        }
        Ok(())
    }

    fn to_body(&self) -> ChatResult<Value> {
        self.checked()?;
        serde_json::to_value(self).map_err(protocol)
    }
}

/// `/v1/schedules{/id}{action}`, with the id checked first.
fn schedule_path(id: &str, action: &str) -> ChatResult<String> {
    validate_prefixed_id(id, "sch_", "tarea")?;
    Ok(format!("/v1/schedules/{id}{action}"))
}

impl HostClient {
    /// Every schedule of the company, in the order the host sends them.
    pub async fn schedules(&self) -> ChatResult<Vec<Schedule>> {
        let list: ScheduleList = self.chat_get("/v1/schedules", &[]).await?;
        Ok(list.schedules)
    }

    pub async fn schedule_create(&self, draft: &ScheduleDraft) -> ChatResult<Schedule> {
        let body = draft.to_body()?;
        let saved: ScheduleSaved = self
            .chat_send_json(Method::POST, "/v1/schedules", &[], &body, true)
            .await?;
        Ok(saved.schedule)
    }

    pub async fn schedule_update(&self, id: &str, draft: &ScheduleDraft) -> ChatResult<Schedule> {
        let path = schedule_path(id, "")?;
        let body = draft.to_body()?;
        let saved: ScheduleSaved = self
            .chat_send_json(Method::PUT, &path, &[], &body, true)
            .await?;
        Ok(saved.schedule)
    }

    /// Deletes a schedule and returns its id as the host confirms it.
    pub async fn schedule_delete(&self, id: &str) -> ChatResult<String> {
        let path = schedule_path(id, "")?;
        let deleted: ScheduleDeleted = self
            .chat_call(Method::DELETE, &path, &[], None, true)
            .await?;
        Ok(deleted.id)
    }

    /// Pauses (`pause`) or resumes (`resume`) a schedule; a resumed one counts from now, not from the pause.
    pub async fn schedule_set_paused(&self, id: &str, paused: bool) -> ChatResult<Schedule> {
        let action = if paused { "/pause" } else { "/resume" };
        let path = schedule_path(id, action)?;
        let saved: ScheduleSaved = self.chat_call(Method::POST, &path, &[], None, true).await?;
        Ok(saved.schedule)
    }

    /// «Lanzar ahora»: starts one run at once without moving the schedule's own rhythm.
    pub async fn schedule_run_now(&self, id: &str) -> ChatResult<ScheduleLaunch> {
        let path = schedule_path(id, "/run_now")?;
        self.chat_call(Method::POST, &path, &[], None, true).await
    }

    /// The last 50 runs of a schedule, newest first, as the host keeps them.
    pub async fn schedule_runs(&self, id: &str) -> ChatResult<Vec<ScheduleRun>> {
        let path = schedule_path(id, "/runs")?;
        let list: RunList = self.chat_get(&path, &[]).await?;
        Ok(list.runs)
    }
}

#[cfg(test)]
mod tests {
    use super::super::error::status_failure;
    use super::*;
    use reqwest::StatusCode;

    fn weekly() -> ScheduleDraft {
        ScheduleDraft {
            title: "Resumen".to_string(),
            conversation_id: "cnv_01HZ".to_string(),
            role: "audio-editor".to_string(),
            body: Some("Haz un resumen.".to_string()),
            recipe_id: None,
            start_date: "2026-11-09".to_string(),
            time: "09:00".to_string(),
            timezone: "Europe/Madrid".to_string(),
            rule: Some("FREQ=WEEKLY;INTERVAL=1;BYDAY=MO".to_string()),
            steps: 1,
            plan_id: None,
        }
    }

    #[test]
    fn schedules_and_runs_read_as_the_host_sends_them() {
        let list: ScheduleList = serde_json::from_str(
            r#"{"ok":true,"schedules":[{"id":"sch_01HZ","title":"Resumen","conversation_id":"cnv_01HZ",
            "role_id":"audio-editor","body":null,"recipe_id":"rcp_01HZ","rule":null,
            "start_date":"2026-11-12","time_local":"09:00","timezone":"Europe/Madrid","steps":1,
            "status":"paused","plan_id":null,"next_run_at":null,"last_run_at":null,
            "created_at":"2026-11-05T10:00:00.000Z","updated_at":"2026-11-05T10:00:00.000Z"}]}"#,
        )
        .expect("schedules");
        assert_eq!(list.schedules[0].status, ScheduleStatus::Paused);
        assert_eq!(list.schedules[0].recipe_id.as_deref(), Some("rcp_01HZ"));
        let runs: RunList = serde_json::from_str(
            r#"{"ok":true,"runs":[{"id":"srn_1","schedule_id":"sch_1","due_at":"2026-11-02T08:00:00.000Z",
            "started_at":null,"outcome":"skipped_cap","delegation_id":null}]}"#,
        )
        .expect("runs");
        assert_eq!(runs.runs[0].outcome, RunOutcome::SkippedCap);
    }

    #[test]
    fn a_run_now_answers_with_a_delegation_or_with_the_skipped_run() {
        let started: ScheduleLaunch = serde_json::from_str(
            r#"{"ok":true,"delegation_id":"dlg_1","status":"queued","depth":1}"#,
        )
        .expect("started");
        assert_eq!(started.delegation_id.as_deref(), Some("dlg_1"));
        assert!(started.run.is_none());
        let skipped: ScheduleLaunch = serde_json::from_str(
            r#"{"ok":true,"run":{"id":"srn_2","schedule_id":"sch_1","due_at":"2026-11-02T08:00:00.000Z",
            "started_at":null,"outcome":"skipped_overlap","delegation_id":null}}"#,
        )
        .expect("skipped");
        assert_eq!(
            skipped.run.map(|run| run.outcome),
            Some(RunOutcome::SkippedOverlap)
        );
    }

    #[test]
    fn a_schedule_id_must_be_plain_before_it_reaches_a_path() {
        assert_eq!(
            schedule_path("sch_01HZ", "/runs").expect("plain id"),
            "/v1/schedules/sch_01HZ/runs"
        );
        assert!(schedule_path("../sch_x", "").is_err());
        assert!(schedule_path("rcp_01HZ", "").is_err());
        assert!(schedule_path("sch_", "").is_err());
    }

    #[test]
    fn a_draft_sends_only_the_fields_it_has() {
        let once = ScheduleDraft {
            rule: None,
            ..weekly()
        }
        .to_body()
        .expect("body");
        assert!(once.get("rule").is_none());
        assert_eq!(once["body"], "Haz un resumen.");
        assert!(once.get("recipe_id").is_none());
        assert!(once.get("plan_id").is_none());

        let from_recipe = ScheduleDraft {
            body: None,
            recipe_id: Some("rcp_01HZ".to_string()),
            plan_id: Some("plan_01HZ".to_string()),
            steps: 4,
            ..weekly()
        }
        .to_body()
        .expect("body");
        assert!(from_recipe.get("body").is_none());
        assert_eq!(from_recipe["recipe_id"], "rcp_01HZ");
        assert_eq!(from_recipe["plan_id"], "plan_01HZ");
        assert_eq!(from_recipe["steps"], 4);
    }

    #[test]
    fn a_draft_with_a_foreign_conversation_or_recipe_id_is_refused_before_it_leaves() {
        let foreign = ScheduleDraft {
            conversation_id: "../cnv".to_string(),
            ..weekly()
        };
        assert!(foreign.to_body().is_err());
        let bad_recipe = ScheduleDraft {
            recipe_id: Some("sch_01HZ".to_string()),
            ..weekly()
        };
        assert!(bad_recipe.to_body().is_err());
    }

    #[test]
    fn an_invalid_schedule_keeps_the_field_that_is_wrong() {
        let body = br#"{"detail":{"code":"invalid_schedule","field":"schedule","message":"rule"}}"#;
        assert_eq!(
            schedule_refusal(body),
            Some(ChatFailure::ScheduleRefused {
                code: "invalid_schedule".to_string(),
                reason: Some("rule".to_string()),
            })
        );
    }

    #[test]
    fn a_limit_or_a_missing_schedule_is_a_schedule_refusal_without_a_reason() {
        for code in ["schedule_limit", "schedule_done", "no_such_schedule"] {
            let body = format!(r#"{{"detail":{{"code":"{code}"}}}}"#);
            assert_eq!(
                schedule_refusal(body.as_bytes()),
                Some(ChatFailure::ScheduleRefused {
                    code: code.to_string(),
                    reason: None,
                })
            );
        }
    }

    #[test]
    fn a_recipe_secret_and_a_plan_refusal_keep_their_own_types() {
        assert_eq!(
            schedule_refusal(br#"{"detail":{"code":"secret_detected","reasons":[]}}"#),
            None
        );
        let plan = status_failure(
            StatusCode::CONFLICT,
            br#"{"detail":{"code":"plan_expired"}}"#,
            None,
            true,
        );
        assert_eq!(
            plan,
            ChatFailure::PlanRefused {
                code: "plan_expired".to_string(),
            }
        );
    }
}
