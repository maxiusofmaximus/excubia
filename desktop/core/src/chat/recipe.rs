//! «Recetas»: the reusable texts of a hired professional, over REST. Listing is `READ_STATE`;
//! saving, editing, deleting, drafting and running are `APPROVE`, which the host answers with
//! `403` to a device without it. Nothing here saves on its own: the operator reviews a draft
//! before it is stored. A recipe id and a delegation id go into a path or body, so they are
//! checked here before they leave the device.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::error::{ChatFailure, ChatResult};
use super::validate_prefixed_id;
use crate::client::HostClient;

/// The codes of a recipe refusal. `plan_not_approved` stays a plan refusal, as the host
/// sends it from the plan routes too; `agent_busy` keeps its own retry wait.
const REFUSAL_CODES: [&str; 8] = [
    "secret_detected",
    "risk_needs_confirmation",
    "invalid_recipe",
    "recipe_limit",
    "recipe_title_taken",
    "no_hire_for_role",
    "delegation_not_done",
    "conversation_has_no_project",
];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Recipe {
    pub id: String,
    pub role_id: String,
    pub title: String,
    pub body: String,
    pub uses: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// The text Alfred drafts from a finished delegation. Nothing is stored until the operator saves it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RecipeDraft {
    pub role_id: String,
    pub title: String,
    pub body: String,
}

/// A run started by a recipe: the delegation the host opened and its state at once.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RecipeRun {
    pub delegation_id: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
struct RecipeList {
    recipes: Vec<Recipe>,
}

#[derive(Debug, Deserialize)]
struct RecipeSaved {
    recipe: Recipe,
}

#[derive(Debug, Deserialize)]
struct DraftBody {
    draft: RecipeDraft,
}

#[derive(Debug, Deserialize)]
struct RecipeDeleted {
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
    reasons: Vec<String>,
}

/// The host's refusal of a recipe route, with the reasons it lists (never the text itself).
pub(super) fn recipe_refusal(body: &[u8]) -> Option<ChatFailure> {
    let detail = serde_json::from_slice::<RefusalBody>(body).ok()?.detail;
    REFUSAL_CODES
        .contains(&detail.code.as_str())
        .then_some(ChatFailure::RecipeRefused {
            code: detail.code,
            reasons: detail.reasons,
        })
}

impl HostClient {
    /// The recipes of one professional, in the order the host sends them.
    pub async fn recipes(&self, role: &str) -> ChatResult<Vec<Recipe>> {
        let list: RecipeList = self
            .chat_get("/v1/recipes", &[("role", role.to_string())])
            .await?;
        Ok(list.recipes)
    }

    /// A draft from a finished delegation. The host answers only for a delegation that is done.
    pub async fn recipe_draft(&self, delegation_id: &str) -> ChatResult<RecipeDraft> {
        validate_prefixed_id(delegation_id, "dlg_", "delegación")?;
        let body = json!({ "delegation_id": delegation_id });
        let drafted: DraftBody = self
            .chat_send_json(Method::POST, "/v1/recipes/draft", &[], &body, true)
            .await?;
        Ok(drafted.draft)
    }

    pub async fn recipe_create(
        &self,
        role: &str,
        title: &str,
        body: &str,
        confirm_risks: bool,
    ) -> ChatResult<Recipe> {
        let body =
            json!({ "role": role, "title": title, "body": body, "confirm_risks": confirm_risks });
        let saved: RecipeSaved = self
            .chat_send_json(Method::POST, "/v1/recipes", &[], &body, true)
            .await?;
        Ok(saved.recipe)
    }

    pub async fn recipe_update(
        &self,
        id: &str,
        title: &str,
        body: &str,
        confirm_risks: bool,
    ) -> ChatResult<Recipe> {
        let path = recipe_path(id, "")?;
        let body = json!({ "title": title, "body": body, "confirm_risks": confirm_risks });
        let saved: RecipeSaved = self
            .chat_send_json(Method::PUT, &path, &[], &body, true)
            .await?;
        Ok(saved.recipe)
    }

    /// Deletes a recipe and returns its id as the host confirms it.
    pub async fn recipe_delete(&self, id: &str) -> ChatResult<String> {
        let path = recipe_path(id, "")?;
        let deleted: RecipeDeleted = self
            .chat_call(Method::DELETE, &path, &[], None, true)
            .await?;
        Ok(deleted.id)
    }

    /// Starts the recipe in a conversation with a project. One step: the host's default.
    pub async fn recipe_run(&self, id: &str, conversation_id: &str) -> ChatResult<RecipeRun> {
        validate_prefixed_id(conversation_id, "cnv_", "conversación")?;
        let path = recipe_path(id, "/run")?;
        let body = json!({ "conversation_id": conversation_id });
        self.chat_send_json(Method::POST, &path, &[], &body, true)
            .await
    }
}

/// `/v1/recipes/{id}{action}`, with the id checked first.
fn recipe_path(id: &str, action: &str) -> ChatResult<String> {
    validate_prefixed_id(id, "rcp_", "receta")?;
    Ok(format!("/v1/recipes/{id}{action}"))
}

#[cfg(test)]
mod tests {
    use super::super::error::status_failure;
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn the_recipes_read_with_their_fields_as_the_host_sends_them() {
        let body = r#"{"ok":true,"recipes":[{"id":"rcp_01HZ","role_id":"audio-editor",
            "title":"Resumen","body":"Haz un resumen.","uses":2,
            "created_at":"2026-11-05T10:00:00.000Z","updated_at":"2026-11-05T11:00:00.000Z"}]}"#;
        let list: RecipeList = serde_json::from_str(body).expect("recipes");
        assert_eq!(list.recipes[0].role_id, "audio-editor");
        assert_eq!(list.recipes[0].uses, 2);
    }

    #[test]
    fn a_draft_and_a_run_read_as_the_host_sends_them() {
        let draft: DraftBody = serde_json::from_str(
            r#"{"ok":true,"draft":{"role_id":"audio-editor","title":"T","body":"B"}}"#,
        )
        .expect("draft");
        assert_eq!(draft.draft.title, "T");
        let run: RecipeRun = serde_json::from_str(
            r#"{"ok":true,"delegation_id":"dlg_01HZ","status":"queued","depth":1}"#,
        )
        .expect("run");
        assert_eq!(run.delegation_id, "dlg_01HZ");
        assert_eq!(run.status, "queued");
    }

    #[test]
    fn a_recipe_id_must_be_plain_before_it_reaches_a_path() {
        assert_eq!(
            recipe_path("rcp_01HZ", "/run").expect("plain id"),
            "/v1/recipes/rcp_01HZ/run"
        );
        assert!(recipe_path("../rcp_x", "").is_err());
        assert!(recipe_path("plan_01HZ", "").is_err());
        assert!(recipe_path("rcp_", "").is_err());
    }

    #[test]
    fn a_secret_refusal_keeps_its_code_and_its_reasons_but_never_the_text() {
        let body =
            r#"{"detail":{"code":"secret_detected","reasons":["parece una clave de API (sk-…)"]}}"#
                .as_bytes();
        assert_eq!(
            recipe_refusal(body),
            Some(ChatFailure::RecipeRefused {
                code: "secret_detected".to_string(),
                reasons: vec!["parece una clave de API (sk-…)".to_string()],
            })
        );
    }

    #[test]
    fn a_risk_refusal_and_a_text_refusal_are_recipe_refusals() {
        for code in [
            "risk_needs_confirmation",
            "invalid_recipe",
            "recipe_title_taken",
            "no_hire_for_role",
        ] {
            let body = format!(r#"{{"detail":{{"code":"{code}"}}}}"#);
            assert!(matches!(
                recipe_refusal(body.as_bytes()),
                Some(ChatFailure::RecipeRefused { code: ref found, .. }) if found == code
            ));
        }
    }

    #[test]
    fn other_codes_and_a_plain_text_detail_are_not_recipe_refusals() {
        assert_eq!(recipe_refusal(br#"{"detail":{"code":"bad_pane"}}"#), None);
        assert_eq!(recipe_refusal(br#"{"detail":"not_found"}"#), None);
        assert_eq!(recipe_refusal(br#"{"detail":{"code":"agent_busy"}}"#), None);
        assert_eq!(recipe_refusal(b"no es json"), None);
    }

    #[test]
    fn a_recipe_conflict_reaches_the_screen_as_a_recipe_refusal_and_a_busy_agent_keeps_its_wait() {
        let delegation = status_failure(
            StatusCode::CONFLICT,
            br#"{"detail":{"code":"delegation_not_done"}}"#,
            None,
            true,
        );
        assert_eq!(
            delegation,
            ChatFailure::RecipeRefused {
                code: "delegation_not_done".to_string(),
                reasons: Vec::new(),
            }
        );
        let busy = status_failure(
            StatusCode::CONFLICT,
            br#"{"detail":{"code":"agent_busy"}}"#,
            Some(30),
            true,
        );
        assert!(matches!(
            busy,
            ChatFailure::AgentBusy {
                retry_after_secs: Some(30),
                ..
            }
        ));
    }

    #[test]
    fn a_plan_refusal_inside_a_run_stays_a_plan_refusal() {
        let failure = status_failure(
            StatusCode::CONFLICT,
            br#"{"detail":{"code":"plan_not_approved"}}"#,
            None,
            true,
        );
        assert!(matches!(failure, ChatFailure::PlanRefused { .. }));
    }
}
