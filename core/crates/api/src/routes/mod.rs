//! Route handlers, one module per surface.

mod actions;
mod agenda;
mod alerts;
mod basic;
mod calendar;
mod calendar_ics;
mod catalog;
mod chat;
mod contacts;
mod devices;
mod events;
mod hires;
mod hooks;
mod mail;
mod mail_send;
mod notes;
mod onboarding;
mod pair;
mod pake;
mod roles;
mod voice;
mod workspaces;

pub use actions::{keys, read, respond, seen, stop};
pub use agenda::agenda_routes;
pub use basic::{health, state};
pub use calendar_ics::{IMPORT_PATH, MAX_ICS_BODY_BYTES};
pub use catalog::list as catalog;
pub use chat::{
    approval_settings as chat_approval_settings, approval_state as chat_approval_state,
    attachment_content as chat_attachment_content, bootstrap as chat_bootstrap,
    cancel_conversation as chat_cancel_conversation, cancel_delegation as chat_cancel_delegation,
    change_approval_settings as chat_change_approval_settings,
    complete_attachment as chat_complete_attachment, consume_approval as chat_consume_approval,
    conversation_counts as chat_conversation_counts, conversations as chat_conversations,
    create as chat_create_conversation, create_approval as chat_create_approval,
    create_attachment as chat_create_attachment, decide_approval as chat_decide_approval,
    delegation as chat_delegation, delete_attachment as chat_delete_attachment,
    delete_conversation as chat_delete_conversation, list_approvals as chat_approvals,
    messages as chat_messages, read as chat_read, sections as chat_sections,
    send as chat_send_message, show as chat_conversation, show_attachment as chat_attachment,
    stop_conversation as chat_stop_conversation, timeline as chat_timeline, tool as chat_tool,
    update as chat_update_conversation, upload_chunk as chat_upload_chunk,
    upsert as chat_upsert_section,
};
pub use devices::{device_sas, grant_device, list_devices, revoke_device};
pub use events::events;
pub use hires::{
    create as create_hire, dismiss as dismiss_hire, list as list_hires, update as update_hire,
};
pub use hooks::report as hook_report;
pub use mail::{
    accounts as mail_accounts, login_start as mail_login_start, login_status as mail_login_status,
};
pub use mail_send::{
    draft_review as mail_draft_review, draft_update as mail_draft_update,
    send_cancel as mail_send_cancel, send_disable as mail_send_disable,
    send_draft as mail_send_draft, send_enable_start as mail_send_enable_start,
    send_enable_status as mail_send_enable_status, send_status as mail_send_status,
};
pub use onboarding::{complete as complete_onboarding, state as onboarding_state};
pub use pair::pair;
pub use pake::{admin_routes as pake_admin_routes, routes as pake_routes};
pub use roles::list as role_catalog;
pub use voice::transcribe as transcribe_voice;
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
