//! The notes Alfred keeps for the user (F2): what the store holds, and the limits
//! the tools enforce on them.

/// The most notes a company keeps; the next save is refused with `note_limit`.
pub const MAX_NOTES: i64 = 2_000;

/// The longest subject, in characters. The body is bounded by `MAX_MESSAGE_BYTES`.
pub const NOTE_SUBJECT_MAX_CHARS: usize = 120;

/// The most results one search returns.
pub const NOTE_SEARCH_MAX_RESULTS: usize = 20;

/// The tool a delete question is filed under; its payload carries the `note_id`.
pub const NOTE_DELETE_TOOL: &str = "note.delete";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub id: String,
    pub subject: Option<String>,
    pub body: String,
    pub conversation_id: Option<String>,
    /// The user message that made the note: the turn's own message, or the last
    /// user message of the conversation when the turn names none.
    pub message_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A search result: the note without its body, which is cut to an excerpt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteHit {
    pub id: String,
    pub subject: Option<String>,
    pub excerpt: String,
    pub updated_at: String,
}
