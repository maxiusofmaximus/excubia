//! The recipes over REST (F5): the bodies a device sends and the JSON it reads.
//! The guard runs in the store (`admit`), so nothing here decides what may be
//! kept; this module only checks the shape of what arrives.

use excubia_chat::{Recipe, RecipeText};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::ApiError;

/// A recipe a device keeps for one oficio. `confirm_risks` is the person's
/// explicit yes to the risks the guard names; it never excuses a secret.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeBody {
    pub role: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub confirm_risks: bool,
}

/// An edit: the whole text again, so the person reviews what is kept.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeUpdateBody {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub confirm_risks: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftBody {
    pub delegation_id: String,
}

/// A run: the conversation it runs in, how many steps, and the plan it belongs to.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunBody {
    pub conversation_id: String,
    #[serde(default = "one_step")]
    pub steps: u32,
    #[serde(default)]
    pub plan_id: Option<String>,
}

fn one_step() -> u32 {
    1
}

impl RecipeBody {
    pub fn text(&self) -> RecipeText {
        RecipeText {
            title: self.title.clone(),
            body: self.body.clone(),
        }
    }
}

impl RecipeUpdateBody {
    pub fn text(&self) -> RecipeText {
        RecipeText {
            title: self.title.clone(),
            body: self.body.clone(),
        }
    }
}

impl RunBody {
    /// A run of zero steps does nothing, so it is refused before the store is asked.
    pub fn checked_steps(&self) -> Result<u32, ApiError> {
        if self.steps == 0 {
            return Err(ApiError::invalid_field(
                "invalid_steps",
                "steps",
                "steps must be at least 1",
            ));
        }
        Ok(self.steps)
    }
}

pub fn recipe_json(recipe: &Recipe) -> Value {
    json!({
        "id": recipe.id,
        "role_id": recipe.role_id,
        "title": recipe.title,
        "body": recipe.body,
        "uses": recipe.uses,
        "created_at": recipe.created_at,
        "updated_at": recipe.updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(steps: u32) -> RunBody {
        RunBody {
            conversation_id: "conv_1".into(),
            steps,
            plan_id: None,
        }
    }

    #[test]
    fn a_run_of_zero_steps_is_refused_at_the_boundary() {
        let error = run(0).checked_steps().expect_err("zero steps");
        assert_eq!(error.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(error.body["detail"]["field"], "steps");
    }

    #[test]
    fn a_run_of_one_step_passes() {
        assert_eq!(run(1).checked_steps().ok(), Some(1));
    }

    #[test]
    fn the_steps_default_to_one_when_absent() {
        let body: RunBody = serde_json::from_str(r#"{"conversation_id":"c"}"#).expect("body");
        assert_eq!(body.steps, 1);
        assert!(body.plan_id.is_none());
    }

    #[test]
    fn an_unknown_field_in_a_recipe_is_refused() {
        let parsed: Result<RecipeBody, _> =
            serde_json::from_str(r#"{"role":"lead","title":"t","body":"b","owner":"x"}"#);
        assert!(parsed.is_err());
    }
}
