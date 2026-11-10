//! The wire shapes of the alert rules (F5): what the app sends, what it gets back, and how a
//! refusal names its field, as the rest of the agenda does. The routes are in `routes/alerts.rs`.

use excubia_core_chat::{field_refusal, AlertMode, NewAlertRule};
use serde::Deserialize;

use crate::error::ApiError;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleBody {
    trigger: String,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    status: Option<String>,
    action: String,
}

impl RuleBody {
    pub fn into_new(self) -> Result<NewAlertRule, ApiError> {
        NewAlertRule::new(
            &self.trigger,
            self.agent.as_deref(),
            self.status.as_deref(),
            &self.action,
        )
        .map_err(field_error)
    }
}

/// An update changes one thing only: whether the rule is on.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnabledBody {
    pub enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeBody {
    mode: String,
}

impl ModeBody {
    pub fn into_mode(self) -> Result<AlertMode, ApiError> {
        AlertMode::parse(&self.mode).ok_or_else(|| field_error("mode"))
    }
}

fn field_error(field: &str) -> ApiError {
    ApiError::invalid_field("invalid_field", field, field_refusal(field))
}
