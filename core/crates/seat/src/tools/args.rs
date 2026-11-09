//! The JSON argument shapes of each tool. Strict: unknown fields are refused by the parser.

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReplyArgs {
    pub(super) text: String,
    #[serde(default)]
    pub(super) suggestions: Vec<String>,
    #[serde(default)]
    pub(super) attachment_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AskArgs {
    pub(super) question: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProgressArgs {
    pub(super) text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectCreateArgs {
    pub(super) name: String,
    #[serde(default)]
    pub(super) path: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AgentSpawnArgs {
    pub(super) name: String,
    #[serde(default)]
    pub(super) role: Option<String>,
    #[serde(default)]
    pub(super) project_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DelegationAssignArgs {
    pub(super) to_agent_id: String,
    pub(super) task: String,
    #[serde(default)]
    pub(super) conversation_id: Option<String>,
    #[serde(default)]
    pub(super) steps: Option<u32>,
    #[serde(default)]
    pub(super) plan_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DelegationCompleteArgs {
    pub(super) delegation_id: String,
    pub(super) result: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoteSaveArgs {
    pub(super) body: String,
    #[serde(default)]
    pub(super) subject: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoteSearchArgs {
    pub(super) query: String,
    #[serde(default)]
    pub(super) limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NoteUpdateArgs {
    pub(super) id: String,
    #[serde(default)]
    pub(super) subject: Option<String>,
    #[serde(default)]
    pub(super) body: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IdArgs {
    pub(super) id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ContactFindArgs {
    pub(super) query: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ContactSaveArgs {
    #[serde(default)]
    pub(super) id: Option<String>,
    pub(super) name: String,
    #[serde(default)]
    pub(super) aliases: Option<Vec<String>>,
    #[serde(default)]
    pub(super) addresses: Option<Vec<String>>,
    #[serde(default)]
    pub(super) notes: Option<String>,
}
