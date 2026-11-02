//! The alert rules over REST (`/v1/alerts`): the rules that say what avisa and the mode they run
//! in. Reads need `READ_STATE`, writes `RESPOND`. Every call is signed like the chat routes.

use std::time::Duration;

use reqwest::Method;
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::Deserialize;
use serde_json::json;

use super::alerts_model::{AlertMode, AlertRule, AlertRuleDraft};
use super::error::{detail_code, protocol, status_failure, ChatFailure, ChatResult};
use super::error_text::alert_text;
use super::validate_prefixed_id;
use crate::client::HostClient;
use crate::error::CoreError;

const RULE_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Deserialize)]
struct RulesEnvelope {
    rules: Vec<AlertRule>,
}

#[derive(Deserialize)]
struct RuleEnvelope {
    rule: AlertRule,
}

#[derive(Deserialize)]
struct ModeEnvelope {
    mode: AlertMode,
}

/// `/v1/alerts/rules/{id}{action}`, with the id checked first.
fn rule_path(id: &str, action: &str) -> ChatResult<String> {
    validate_prefixed_id(id, "ar_", "regla")?;
    Ok(format!("/v1/alerts/rules/{id}{action}"))
}

/// The host names a rule refusal by its bare code in `detail`. A code that is not one of
/// ours keeps the generic meaning of its status.
fn rule_refusal(body: &[u8]) -> Option<ChatFailure> {
    let code = detail_code(body)?;
    let message = alert_text(&code)?;
    Some(ChatFailure::AlertRefused {
        code,
        message: message.to_string(),
    })
}

impl HostClient {
    pub async fn alert_rules(&self) -> ChatResult<Vec<AlertRule>> {
        let envelope: RulesEnvelope = self
            .alert_call(Method::GET, "/v1/alerts/rules", None, false)
            .await?;
        Ok(envelope.rules)
    }

    pub async fn alert_rule_create(&self, draft: &AlertRuleDraft) -> ChatResult<AlertRule> {
        let body = serde_json::to_vec(draft).map_err(protocol)?;
        let envelope: RuleEnvelope = self
            .alert_call(Method::POST, "/v1/alerts/rules", Some(&body), true)
            .await?;
        Ok(envelope.rule)
    }

    pub async fn alert_rule_set_enabled(&self, id: &str, enabled: bool) -> ChatResult<AlertRule> {
        let path = rule_path(id, "/update")?;
        let body = serde_json::to_vec(&json!({ "enabled": enabled })).map_err(protocol)?;
        let envelope: RuleEnvelope = self
            .alert_call(Method::POST, &path, Some(&body), true)
            .await?;
        Ok(envelope.rule)
    }

    pub async fn alert_rule_delete(&self, id: &str) -> ChatResult<()> {
        let path = rule_path(id, "/delete")?;
        self.alert_call::<IgnoredAny>(Method::POST, &path, Some(b"{}"), true)
            .await?;
        Ok(())
    }

    pub async fn alert_mode(&self) -> ChatResult<AlertMode> {
        let envelope: ModeEnvelope = self
            .alert_call(Method::GET, "/v1/alerts/mode", None, false)
            .await?;
        Ok(envelope.mode)
    }

    pub async fn alert_mode_set(&self, mode: AlertMode) -> ChatResult<AlertMode> {
        let body = serde_json::to_vec(&json!({ "mode": mode })).map_err(protocol)?;
        let envelope: ModeEnvelope = self
            .alert_call(Method::POST, "/v1/alerts/mode", Some(&body), true)
            .await?;
        Ok(envelope.mode)
    }

    /// One signed call. A rule refusal is read before the generic status mapping, because
    /// `not_found` here is a rule, not a conversation.
    async fn alert_call<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&[u8]>,
        write: bool,
    ) -> ChatResult<T> {
        let response = self
            .signed_call(method, path, &[], body)
            .map_err(|error| ChatFailure::from_core(&error, write))?
            .timeout(RULE_TIMEOUT)
            .send()
            .await
            .map_err(|error| ChatFailure::from_core(&CoreError::from(error), write))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| ChatFailure::from_core(&CoreError::from(error), write))?;
        if status.is_success() {
            return serde_json::from_slice(&bytes).map_err(protocol);
        }
        Err(rule_refusal(&bytes).unwrap_or_else(|| status_failure(status, &bytes, None, write)))
    }
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

    use super::*;

    #[test]
    fn a_rule_id_must_carry_its_prefix() {
        assert!(rule_path("nt_1", "/update").is_err());
        assert_eq!(
            rule_path("ar_01JAB", "/delete").unwrap(),
            "/v1/alerts/rules/ar_01JAB/delete"
        );
    }

    #[test]
    fn a_duplicate_rule_is_refused_in_words() {
        let failure = rule_refusal(br#"{"ok":false,"detail":"rule_exists"}"#).unwrap();
        assert!(matches!(
            failure,
            ChatFailure::AlertRefused { ref code, ref message }
                if code == "rule_exists" && message == "Ya existe una regla igual"
        ));
    }

    #[test]
    fn a_field_refusal_is_not_a_rule_refusal() {
        let body = br#"{"ok":false,"detail":{"code":"invalid_field","field":"status"}}"#;
        assert!(rule_refusal(body).is_none());
        assert!(matches!(
            status_failure(StatusCode::UNPROCESSABLE_ENTITY, body, None, true),
            ChatFailure::Invalid { field: Some(field), .. } if field == "status"
        ));
    }
}
