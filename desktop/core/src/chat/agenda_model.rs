//! The agenda's wire shapes: the notes and contacts the app keeps on the host.
//! Field names follow `docs/contracts/api.md`, «Agenda de la app».

use serde::{Deserialize, Serialize};

/// One row of the notes list: the subject, a line of the body and when it changed.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NoteSummary {
    pub id: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub excerpt: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NotePage {
    #[serde(default)]
    pub notes: Vec<NoteSummary>,
    /// The `before` of the next page; `None` on the last one.
    #[serde(default)]
    pub next: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Note {
    pub id: String,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// What the screen sends for a new note. `title: None` leaves the subject out;
/// `message_id` links the note to a chat message the host already has.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NoteDraft {
    pub title: Option<String>,
    pub body: String,
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContactAddress {
    pub address: String,
    /// `app`, `phone`, `alfred` or `user_message`.
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Contact {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub addresses: Vec<ContactAddress>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ContactPage {
    #[serde(default)]
    pub contacts: Vec<Contact>,
    /// The `after` of the next page; `None` on the last one.
    #[serde(default)]
    pub next: Option<String>,
}

/// A contact as the screen sends it. Every field goes on each write, so an
/// update replaces the lists it shows and keeps nothing it does not show.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ContactDraft {
    pub name: String,
    pub aliases: Vec<String>,
    pub addresses: Vec<String>,
    pub notes: String,
}
