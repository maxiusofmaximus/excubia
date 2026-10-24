//! Route handlers, one module per surface.

mod actions;
mod basic;
mod catalog;
mod chat;
mod devices;
mod events;
mod hooks;
mod pair;
mod workspaces;

pub use actions::{keys, read, respond, seen, stop};
pub use basic::{health, state};
pub use catalog::list as catalog;
pub use chat::{
    approval_settings as chat_approval_settings, approval_state as chat_approval_state,
    bootstrap as chat_bootstrap, cancel_conversation as chat_cancel_conversation,
    cancel_delegation as chat_cancel_delegation,
    change_approval_settings as chat_change_approval_settings,
    consume_approval as chat_consume_approval, conversations as chat_conversations,
    create as chat_create_conversation, create_approval as chat_create_approval,
    decide_approval as chat_decide_approval, delegation as chat_delegation,
    list_approvals as chat_approvals, messages as chat_messages, read as chat_read,
    sections as chat_sections, send as chat_send_message, show as chat_conversation,
    timeline as chat_timeline, tool as chat_tool, update as chat_update_conversation,
    upsert as chat_upsert_section,
};
pub use devices::{list_devices, revoke_device};
pub use events::events;
pub use hooks::report as hook_report;
pub use pair::pair;
pub use workspaces::create as create_workspace;

use axum::body::Bytes;
use serde::de::DeserializeOwned;

use crate::error::ApiError;

/// Parse the exact bytes that were signed. The contract does not require a
/// content type, so this never looks at one: a correctly signed body sent
/// without `application/json` is still valid.
pub(crate) fn parse_body<T: DeserializeOwned>(bytes: &Bytes) -> Result<T, ApiError> {
    let bytes = if bytes.is_empty() {
        b"{}".as_slice()
    } else {
        bytes.as_ref()
    };
    serde_json::from_slice(bytes).map_err(|error| match error.classify() {
        serde_json::error::Category::Data => ApiError::invalid_body_model(error.to_string()),
        _ => ApiError::invalid_body(),
    })
}
