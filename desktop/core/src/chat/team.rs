//! Delegations: the record behind a timeline card, its full detail and the
//! two cancellation routes. Cards themselves stay on the timeline contract.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::error::ChatResult;
use super::model::{CardLinks, Message};
use super::validate_prefixed_id;
use crate::client::GatewayClient;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Delegation {
    pub id: String,
    #[serde(default)]
    pub conversation_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub from_agent_id: String,
    #[serde(default)]
    pub from_name: String,
    #[serde(default)]
    pub to_agent_id: String,
    #[serde(default)]
    pub to_name: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub depth: i64,
    #[serde(default)]
    pub turn_key: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub result_message_id: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DelegationChild {
    pub id: String,
    #[serde(default)]
    pub depth: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DelegationCard {
    pub id: String,
    #[serde(default)]
    pub seq: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub pane_id: Option<String>,
    #[serde(default)]
    pub attention: i64,
    #[serde(default)]
    pub links: Option<CardLinks>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Synthesis {
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DelegationDetail {
    pub delegation: Delegation,
    #[serde(default)]
    pub children: Vec<DelegationChild>,
    #[serde(default)]
    pub card: Option<DelegationCard>,
    #[serde(default)]
    pub result_message: Option<Message>,
    #[serde(default)]
    pub synthesis: Option<Synthesis>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CancelledConversation {
    #[serde(default)]
    pub conversation_id: String,
    #[serde(default)]
    pub cancelled: Vec<String>,
}

impl GatewayClient {
    pub async fn chat_delegation(&self, id: &str) -> ChatResult<DelegationDetail> {
        validate_prefixed_id(id, "dlg_", "delegación")?;
        self.chat_get(&format!("/v1/chat/delegations/{id}"), &[])
            .await
    }

    pub async fn chat_cancel_delegation(&self, id: &str) -> ChatResult<DelegationDetail> {
        validate_prefixed_id(id, "dlg_", "delegación")?;
        self.chat_send_json(
            Method::POST,
            &format!("/v1/chat/delegations/{id}/cancel"),
            &[],
            &json!({}),
            true,
        )
        .await
    }

    /// The kill switch of a conversation: cancel every active delegation and
    /// keep the conversation and its history.
    pub async fn chat_cancel_conversation(
        &self,
        conversation_id: &str,
    ) -> ChatResult<CancelledConversation> {
        validate_prefixed_id(conversation_id, "cnv_", "conversación")?;
        self.chat_send_json(
            Method::POST,
            &format!("/v1/chat/conversations/{conversation_id}/cancel"),
            &[],
            &json!({}),
            true,
        )
        .await
    }
}
