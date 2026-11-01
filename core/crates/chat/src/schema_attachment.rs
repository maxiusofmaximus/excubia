//! The attachments of a chat (version 13): one row per file the user adds to a
//! message. A row is `uploading` while its bytes arrive and `ready` once the
//! final content is stored; `message_id` stays NULL until a message takes it.
//! Deleting the message unlinks the row again, so the user can send it anew.

pub const ATTACHMENT_SCHEMA: &str = r#"
CREATE TABLE attachment (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    client_id       TEXT NOT NULL,
    name            TEXT NOT NULL,
    declared_size   INTEGER NOT NULL,
    declared_sha256 TEXT NOT NULL,
    state           TEXT NOT NULL CHECK (state IN ('uploading', 'ready')),
    mime            TEXT NULL,
    width           INTEGER NULL,
    height          INTEGER NULL,
    final_size      INTEGER NULL,
    final_sha256    TEXT NULL,
    message_id      TEXT NULL REFERENCES message(id) ON DELETE SET NULL,
    created_at      TEXT NOT NULL,
    UNIQUE(conversation_id, client_id)
);

CREATE INDEX attachment_message ON attachment(message_id);
"#;

#[cfg(test)]
mod tests {
    use hn_core_test_dir::TestDir;

    use crate::schema::{open, open_in_memory, SCHEMA_VERSION};

    /// A version-12 database has no attachment table; the next open creates it
    /// and keeps the conversations that were already there.
    #[test]
    fn a_version_twelve_database_gains_the_attachment_table_and_keeps_its_rows() {
        let dir = TestDir::new("schema-v12");
        let path = dir.join("chat.db");
        {
            let (connection, _) = open(&path).expect("current database");
            connection
                .execute_batch(
                    "INSERT INTO conversation
                        (id, company_id, title, last_message_at, created_at, updated_at)
                     SELECT 'cnv_legacy', id, 'Legacy', '2026-10-22T09:00:00.000Z',
                            '2026-10-22T09:00:00.000Z', '2026-10-22T09:00:00.000Z'
                       FROM company;
                     DROP TABLE attachment;",
                )
                .expect("a version-twelve layout");
            connection
                .pragma_update(None, "user_version", 12)
                .expect("version twelve");
        }

        let (connection, _) = open(&path).expect("migrated");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, SCHEMA_VERSION);
        let title: String = connection
            .query_row(
                "SELECT title FROM conversation WHERE id = 'cnv_legacy'",
                [],
                |row| row.get(0),
            )
            .expect("old conversation");
        assert_eq!(title, "Legacy");
        let attachments: i64 = connection
            .query_row("SELECT COUNT(*) FROM attachment", [], |row| row.get(0))
            .expect("attachment table");
        assert_eq!(attachments, 0);
        drop(connection);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Deleting a conversation takes its attachment rows with it.
    #[test]
    fn deleting_a_conversation_cascades_to_its_attachments() {
        let (connection, _) = open_in_memory().expect("open");
        connection
            .execute_batch(
                "INSERT INTO conversation
                    (id, company_id, title, last_message_at, created_at, updated_at)
                 SELECT 'cnv_1', id, 'Chat', '2026-10-22T09:00:00.000Z',
                        '2026-10-22T09:00:00.000Z', '2026-10-22T09:00:00.000Z'
                   FROM company;
                 INSERT INTO attachment
                    (id, conversation_id, client_id, name, declared_size, declared_sha256,
                     state, created_at)
                 VALUES ('att_1', 'cnv_1', 'c1', 'a.png', 1, 'aa', 'uploading',
                         '2026-10-22T09:00:00.000Z');",
            )
            .expect("a conversation with an attachment");
        connection
            .execute("DELETE FROM conversation WHERE id = 'cnv_1'", [])
            .expect("delete");
        let left: i64 = connection
            .query_row("SELECT COUNT(*) FROM attachment", [], |row| row.get(0))
            .expect("rows");
        assert_eq!(left, 0);
    }

    /// Withdrawing a message (`withdraw_turn`) unlinks its attachments, which
    /// keep their files and can be sent again.
    #[test]
    fn deleting_a_message_unlinks_its_attachments() {
        let (connection, _) = open_in_memory().expect("open");
        connection
            .execute_batch(
                "INSERT INTO conversation
                    (id, company_id, title, last_message_at, created_at, updated_at)
                 SELECT 'cnv_1', id, 'Chat', '2026-10-22T09:00:00.000Z',
                        '2026-10-22T09:00:00.000Z', '2026-10-22T09:00:00.000Z'
                   FROM company;
                 INSERT INTO message
                    (id, conversation_id, seq, author_kind, body_md, state, created_at)
                 VALUES ('msg_1', 'cnv_1', 1, 'user', 'hola', 'accepted',
                         '2026-10-22T09:00:00.000Z');
                 INSERT INTO attachment
                    (id, conversation_id, client_id, name, declared_size, declared_sha256,
                     state, message_id, created_at)
                 VALUES ('att_1', 'cnv_1', 'c1', 'a.png', 1, 'aa', 'ready', 'msg_1',
                         '2026-10-22T09:00:00.000Z');",
            )
            .expect("a message with an attachment");
        connection
            .execute("DELETE FROM message WHERE id = 'msg_1'", [])
            .expect("delete message");
        let message: Option<String> = connection
            .query_row(
                "SELECT message_id FROM attachment WHERE id = 'att_1'",
                [],
                |row| row.get(0),
            )
            .expect("row survives");
        assert_eq!(message, None);
    }
}
