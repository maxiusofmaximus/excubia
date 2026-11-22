//! The schemas of Alfred's alert tools (F5). Alfred only; the host enforces who may call them.

use serde_json::{json, Value};

/// The alert tools, in the order the surface lists them.
pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "alert.rule_create",
            "description": "Alfred only. Add a rule that decides which changes alert the user's phone. trigger pane_status needs a status (blocked or done) and may name an agent; trigger calendar_reminder names neither. action notify alerts; action mute never alerts for that change and wins over notify. Without a matching rule the mode decides. An identical rule is refused with rule_exists; at most 100 rules.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "trigger": { "type": "string", "enum": ["pane_status", "calendar_reminder"], "description": "What the rule watches." },
                    "agent": { "type": "string", "minLength": 1, "maxLength": 200, "description": "The agent's name, for pane_status only. Optional." },
                    "status": { "type": "string", "enum": ["blocked", "done"], "description": "The status, for pane_status only." },
                    "action": { "type": "string", "enum": ["notify", "mute"], "description": "notify alerts the phone; mute silences the change." }
                },
                "required": ["trigger", "action"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "alert.rule_list",
            "description": "Alfred only. The alert rules, each with its id, what it watches, its action and whether it is on. Read them before you create or change a rule.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "alert.rule_update",
            "description": "Alfred only. Turn one alert rule on or off, by the id from alert.rule_list. Nothing else of a rule changes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "minLength": 1, "maxLength": 200, "description": "The rule's id from alert.rule_list." },
                    "enabled": { "type": "boolean", "description": "True to turn the rule on, false to turn it off." }
                },
                "required": ["id", "enabled"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "alert.rule_delete",
            "description": "Alfred only. Delete one alert rule by the id from alert.rule_list. This is reversible: a rule can be created again.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "minLength": 1, "maxLength": 200, "description": "The rule's id from alert.rule_list." }
                },
                "required": ["id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "alert.mode_set",
            "description": "Alfred only. How alerts run when no rule decides. all: a blocked or done agent alerts as it always did. rules_only: only the changes that a notify rule names alert. Mute rules apply in both modes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mode": { "type": "string", "enum": ["all", "rules_only"], "description": "The alert mode." }
                },
                "required": ["mode"],
                "additionalProperties": false
            }
        }),
    ]
}
