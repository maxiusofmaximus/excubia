//! The progress line of a run (version 12): what Alfred is doing right now, set
//! by `chat.progress` and cleared when the run closes. Rows written before the
//! version keep NULL in both columns.

pub const RUN_PROGRESS_SCHEMA: &str = r#"
ALTER TABLE run ADD COLUMN progress TEXT NULL;
ALTER TABLE run ADD COLUMN progress_at TEXT NULL;
"#;
