use serde::Deserialize;
use serde_json::Value;

/// The body the hook or the plugin sends to open an approval.
#[derive(Deserialize)]
pub(super) struct CreateBody {
    pub(super) kind: String,
    pub(super) summary: String,
    pub(super) action_digest: String,
    #[serde(default)]
    pub(super) tool: Option<String>,
    #[serde(default)]
    pub(super) pane_id: Option<String>,
    /// The CLI's own label for the agent (OpenCode names its own agents).
    #[serde(default)]
    pub(super) agent_id: Option<String>,
    #[serde(default)]
    pub(super) run_id: Option<String>,
    #[serde(default)]
    pub(super) conversation_id: Option<String>,
    #[serde(default)]
    pub(super) cwd: Option<String>,
    #[serde(default)]
    pub(super) paths: Vec<String>,
    #[serde(default)]
    pub(super) payload_redacted: Option<Value>,
    #[serde(default)]
    pub(super) ttl_seconds: Option<u64>,
    #[serde(default)]
    pub(super) risk: Option<String>,
}

/// The body of the operator's answer.
#[derive(Deserialize)]
pub(super) struct DecisionBody {
    pub(super) decision: String,
    pub(super) client_id: String,
    #[serde(default)]
    pub(super) scope: Option<String>,
    /// The messages of a mail batch the owner leaves out (F2C1).
    #[serde(default)]
    pub(super) exclude: Vec<String>,
}

/// The body of the consumption. The digest is optional: when it is present it
/// must be the one the approval was opened with.
#[derive(Deserialize)]
pub(super) struct ConsumeBody {
    #[serde(default)]
    pub(super) action_digest: Option<String>,
}
