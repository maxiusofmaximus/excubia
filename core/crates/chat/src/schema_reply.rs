//! The reply fields of a message (version 11): the quick replies Alfred offers
//! under its own message, and the message a user message answers. Rows written
//! before the version keep NULL in all three columns.

pub const MESSAGE_REPLY_SCHEMA: &str = r#"
ALTER TABLE message ADD COLUMN suggestions_json TEXT NULL;
ALTER TABLE message ADD COLUMN reply_to_kind TEXT NULL;
ALTER TABLE message ADD COLUMN reply_to_id TEXT NULL;
"#;
