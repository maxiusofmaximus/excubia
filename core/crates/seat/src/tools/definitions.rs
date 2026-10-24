//! The `tools/list` result. The schemas are the contract the model sees;
//! the host still enforces role scope on every call.

use serde_json::{json, Value};

/// One bounded text argument.
fn string_property(description: &str) -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "maxLength": 32768,
        "description": description
    })
}

pub fn definitions() -> Value {
    json!({
        "tools": [
            {
                "name": "chat.reply",
                "description": "Send Alfred's final answer to the chat. This is the only way a chat turn can complete successfully.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "text": string_property("The complete answer, in the user's language.") },
                    "required": ["text"],
                    "additionalProperties": false
                }
            },
            {
                "name": "chat.ask_user",
                "description": "Post a clarification question to the chat. It does not close the turn; finish with chat.reply.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "question": string_property("One concrete question for the user.") },
                    "required": ["question"],
                    "additionalProperties": false
                }
            },
            {
                "name": "project.create",
                "description": "Create a project under workspaces_root and start its lead panel. Alfred only.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1, "maxLength": 80 },
                        "path": { "type": "string", "minLength": 1, "maxLength": 200, "description": "Optional path relative to workspaces_root." }
                    },
                    "required": ["name"],
                    "additionalProperties": false
                }
            },
            {
                "name": "agent.spawn",
                "description": "Create a project seat and start its CLI panel. Leads spawn specialists of their own project.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "minLength": 1, "maxLength": 80 },
                        "role": { "type": "string", "enum": ["lead", "specialist"] },
                        "project_id": { "type": "string", "minLength": 1, "maxLength": 200 }
                    },
                    "required": ["name"],
                    "additionalProperties": false
                }
            },
            {
                "name": "delegation.assign",
                "description": "Assign one task to a lead or specialist in the same project. Alfred assigns leads; a lead assigns specialists.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "to_agent_id": { "type": "string", "minLength": 1, "maxLength": 200 },
                        "task": string_property("The task, at most one line for the card."),
                        "conversation_id": { "type": "string", "minLength": 1, "maxLength": 200 }
                    },
                    "required": ["to_agent_id", "task"],
                    "additionalProperties": false
                }
            },
            {
                "name": "delegation.complete",
                "description": "Finish the delegation assigned to this seat with its full result.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "delegation_id": { "type": "string", "minLength": 1, "maxLength": 200 },
                        "result": string_property("The complete result.")
                    },
                    "required": ["delegation_id", "result"],
                    "additionalProperties": false
                }
            },
            {
                "name": "approval.request",
                "description": "Ask the operator for permission before a risky action. The call blocks until the operator decides; it never decides by itself.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string", "enum": ["command", "network", "edit", "tool"] },
                        "tool": { "type": "string", "minLength": 1, "maxLength": 200 },
                        "summary": string_property("What the action does, in one line."),
                        "args": { "type": "object", "description": "The arguments of the action; they are hashed into the digest." },
                        "cwd": { "type": "string", "minLength": 1, "maxLength": 200 },
                        "paths": { "type": "array", "items": { "type": "string" } },
                        "ttl_seconds": { "type": "integer", "minimum": 1, "maximum": 300 }
                    },
                    "required": ["kind", "tool", "summary"],
                    "additionalProperties": false
                }
            },
            {
                "name": "approval.decide",
                "description": "Decide an approval a worker of your scope asked for. Allowed only for approvals waiting at your level, never for your own, never for a rule that only the operator decides. Each call is audited; a budget of 6 decisions per hour applies.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "approval_id": { "type": "string", "minLength": 1, "maxLength": 200 },
                        "decision": { "type": "string", "enum": ["approve", "deny"] },
                        "reason": string_property("Why you decide it, in one line (optional).")
                    },
                    "required": ["approval_id", "decision"],
                    "additionalProperties": false
                }
            },
            {
                "name": "approval.pending",
                "description": "List the approvals waiting at your level: your project's for a lead, the company's for Alfred. Read-only; it decides nothing and it takes no arguments.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "additionalProperties": false
                }
            }
        ]
    })
}
