//! Persistent chat with Alfred: the `/v1/chat` wire contract and its errors.
//!
//! `model` owns the wire shapes, `client` the signed calls, `error` the typed
//! failures the screen draws. This module only keeps the patch model and the
//! identifier validation both sides share.

mod approvals;
mod client;
mod error;
mod model;
mod team;
#[cfg(test)]
mod tests;

pub use approvals::*;
pub use client::CHAT_PAGE_LIMIT;
pub(crate) use error::protocol;
pub use error::{delegation_text, ChatFailure, ChatResult};
pub use model::*;
pub use team::*;

use serde_json::{Map, Value};

/// `Keep` must stay distinguishable from `Clear`: an absent `section_id`
/// keeps the current section, an explicit `null` removes it.
#[derive(Debug, Clone, Default)]
pub enum SectionChange {
    #[default]
    Keep,
    Clear,
    Set(String),
}

#[derive(Debug, Clone, Default)]
pub struct ConversationPatch {
    pub title: Option<String>,
    pub pinned: Option<bool>,
    pub section: SectionChange,
    pub archived: Option<bool>,
}

impl ConversationPatch {
    fn to_body(&self) -> ChatResult<Vec<u8>> {
        let mut body = Map::new();
        if let Some(title) = self.title.as_deref() {
            body.insert("title".to_string(), Value::String(title.to_string()));
        }
        if let Some(pinned) = self.pinned {
            body.insert("pinned".to_string(), Value::Bool(pinned));
        }
        if let Some(archived) = self.archived {
            body.insert("archived".to_string(), Value::Bool(archived));
        }
        match &self.section {
            SectionChange::Keep => {}
            SectionChange::Clear => {
                body.insert("section_id".to_string(), Value::Null);
            }
            SectionChange::Set(id) => {
                validate_prefixed_id(id, "sec_", "sección")?;
                body.insert("section_id".to_string(), Value::String(id.clone()));
            }
        }
        serde_json::to_vec(&body).map_err(protocol)
    }
}
pub fn validate_client_id(client_id: &str) -> ChatResult<()> {
    if client_id.is_empty() || client_id.len() > 128 {
        return Err(ChatFailure::Invalid {
            message: "El identificador del envío debe tener entre 1 y 128 caracteres.".to_string(),
            field: Some("client_id".to_string()),
        });
    }
    if client_id.chars().any(char::is_control) {
        return Err(ChatFailure::Invalid {
            message: "El identificador del envío contiene caracteres no válidos.".to_string(),
            field: Some("client_id".to_string()),
        });
    }
    Ok(())
}

pub fn validate_prefixed_id(id: &str, prefix: &str, label: &str) -> ChatResult<()> {
    let valid = id.strip_prefix(prefix).is_some_and(|rest| {
        !rest.is_empty()
            && rest.len() <= 40
            && rest
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
    });
    if valid {
        Ok(())
    } else {
        Err(ChatFailure::Invalid {
            message: format!("El identificador de {label} no es válido."),
            field: None,
        })
    }
}
