//! The `alert.*` tools (F5): Alfred keeps the rules that decide which changes alert the user's
//! phone, and the mode they run in. The parser checks every field; the host decides who may
//! call a tool. No tool asks for approval: a rule is reversible and only changes what alerts.

use hn_core_chat::{field_refusal, AlertMode, NewAlertRule, ToolError, ToolRequest};
use serde::Deserialize;

use super::args::IdArgs;
use super::parse::{arguments, name_field, no_arguments};

#[derive(Clone, Debug, PartialEq)]
pub enum AlertCall {
    RuleCreate(NewAlertRule),
    RuleList,
    RuleUpdate { id: String, enabled: bool },
    RuleDelete { id: String },
    ModeSet(AlertMode),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleCreateArgs {
    trigger: String,
    agent: Option<String>,
    status: Option<String>,
    action: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleUpdateArgs {
    id: String,
    enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModeArgs {
    mode: String,
}

pub(super) fn rule_create(request: &ToolRequest) -> Result<AlertCall, ToolError> {
    let args: RuleCreateArgs = arguments(request, "trigger")?;
    let rule = NewAlertRule::new(
        &args.trigger,
        args.agent.as_deref(),
        args.status.as_deref(),
        &args.action,
    )
    .map_err(|field| ToolError::invalid("invalid_arguments", field, field_refusal(field)))?;
    Ok(AlertCall::RuleCreate(rule))
}

pub(super) fn rule_list(request: &ToolRequest) -> Result<AlertCall, ToolError> {
    no_arguments(request)?;
    Ok(AlertCall::RuleList)
}

pub(super) fn rule_update(request: &ToolRequest) -> Result<AlertCall, ToolError> {
    let args: RuleUpdateArgs = arguments(request, "id")?;
    Ok(AlertCall::RuleUpdate {
        id: name_field("id", &args.id)?,
        enabled: args.enabled,
    })
}

pub(super) fn rule_delete(request: &ToolRequest) -> Result<AlertCall, ToolError> {
    let args: IdArgs = arguments(request, "id")?;
    Ok(AlertCall::RuleDelete {
        id: name_field("id", &args.id)?,
    })
}

pub(super) fn mode_set(request: &ToolRequest) -> Result<AlertCall, ToolError> {
    let args: ModeArgs = arguments(request, "mode")?;
    let mode = AlertMode::parse(&args.mode).ok_or_else(|| {
        ToolError::invalid(
            "invalid_arguments",
            "mode",
            "mode must be \"all\" or \"rules_only\"",
        )
    })?;
    Ok(AlertCall::ModeSet(mode))
}
