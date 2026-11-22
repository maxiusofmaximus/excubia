//! The mail sign-in card (MAIL1D §2). It takes its `seq` from the conversation
//! like a message or a card, and it lives only while its login is open. It
//! holds what the user types (the user code) and the page to type it on, never
//! a device code or a token.

pub const MAIL_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS timeline_mail_card (
    id               TEXT PRIMARY KEY,
    conversation_id  TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    login_id         TEXT NOT NULL UNIQUE,
    seq              INTEGER NOT NULL,
    account          TEXT NOT NULL,
    user_code        TEXT NOT NULL,
    verification_uri TEXT NOT NULL,
    created_at       TEXT NOT NULL,
    UNIQUE(conversation_id, seq)
);
"#;

/// The instant the card's login stops being valid (version 8). Cards are taken
/// away on restart, so the rows that predate the column hold nothing to keep.
pub const MAIL_CARD_EXPIRY_SCHEMA: &str = r#"
ALTER TABLE timeline_mail_card ADD COLUMN expires_at TEXT NOT NULL DEFAULT '';
"#;

/// The draft card (version 10): what a conversation shows of a draft, updated in
/// place. It holds the recipients, the subject and a short preview, never the body.
pub const MAIL_DRAFT_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS timeline_mail_draft (
    id               TEXT PRIMARY KEY,
    conversation_id  TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    draft_id         TEXT NOT NULL,
    seq              INTEGER NOT NULL,
    account          TEXT NOT NULL,
    to_json          TEXT NOT NULL,
    subject          TEXT NOT NULL,
    preview          TEXT NOT NULL,
    created_at       TEXT NOT NULL,
    UNIQUE(conversation_id, draft_id),
    UNIQUE(conversation_id, seq)
);
CREATE INDEX IF NOT EXISTS timeline_mail_draft_by_draft ON timeline_mail_draft(draft_id);
"#;
