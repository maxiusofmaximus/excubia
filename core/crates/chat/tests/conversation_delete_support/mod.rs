//! Fixtures for the conversation delete tests: a store with one conversation
//! that owns a row of every table, and a second one that must survive.

use std::path::PathBuf;

use hn_core_chat::{AttachmentFiles, ChatStore, NewConversation};
use hn_core_test_dir::TestDir;
use rusqlite::{params, Connection};

pub const DEVICE: &str = "dev_owner";
const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const NOW: &str = "2026-10-28T10:00:00.000Z";

pub struct Fixture {
    pub store: ChatStore,
    pub db: PathBuf,
    pub dir: TestDir,
    pub alfred: String,
    pub conversation: String,
}

pub fn start(name: &str) -> Fixture {
    let dir = TestDir::new(&format!("core-conv-delete-{name}"));
    let db = dir.join("chat.db");
    let store = ChatStore::open(&db).expect("open store");
    store.set_attachments_root(&dir.join("attachments"));
    let alfred = store.alfred_id().expect("alfred");
    let conversation = new_conversation(&store, "Presupuesto Demo");
    Fixture {
        store,
        db,
        dir,
        alfred,
        conversation,
    }
}

pub fn new_conversation(store: &ChatStore, title: &str) -> String {
    store
        .create_conversation(NewConversation {
            title: Some(title.to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

pub fn second_connection(db: &PathBuf) -> Connection {
    let connection = Connection::open(db).expect("second connection");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys");
    connection
}

/// The tables that have a `conversation_id` column, found in the schema itself.
fn tables_with_conversation_id(connection: &Connection) -> Vec<String> {
    connection
        .prepare(
            "SELECT m.name FROM sqlite_master m, pragma_table_info(m.name) p
              WHERE m.type = 'table' AND p.name = 'conversation_id' ORDER BY m.name",
        )
        .expect("schema")
        .query_map([], |row| row.get(0))
        .expect("tables")
        .collect::<Result<_, _>>()
        .expect("collect")
}

/// The rows of every such table that name `conversation`.
pub fn counts(connection: &Connection, conversation: &str) -> Vec<(String, i64)> {
    tables_with_conversation_id(connection)
        .into_iter()
        .map(|table| {
            let rows = connection
                .query_row(
                    &format!("SELECT COUNT(*) FROM \"{table}\" WHERE conversation_id = ?1"),
                    [conversation],
                    |row| row.get(0),
                )
                .expect("count");
            (table, rows)
        })
        .collect()
}

fn insert_message(connection: &Connection, conversation: &str, id: &str, seq: i64) {
    connection
        .execute(
            "INSERT INTO message (id, conversation_id, seq, author_kind, body_md, state, created_at)
             VALUES (?1, ?2, ?3, 'user', 'Texto del presupuesto', 'done', ?4)",
            params![id, conversation, seq, NOW],
        )
        .expect("message");
}

fn insert_run(connection: &Connection, id: &str, agent: &str, message: Option<&str>, state: &str) {
    connection
        .execute(
            "INSERT INTO run (id, agent_id, message_id, state, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, agent, message, state, NOW],
        )
        .expect("run");
}

/// One row of every kind a conversation can own, so the delete has to reach each.
pub fn populate(fixture: &Fixture, conversation: &str) {
    let connection = second_connection(&fixture.db);
    let company: String = connection
        .query_row("SELECT id FROM company", [], |row| row.get(0))
        .expect("company");
    let alfred = fixture.alfred.as_str();
    insert_message(&connection, conversation, "msg_1", 1);
    insert_run(&connection, "run_1", alfred, Some("msg_1"), "done");
    insert_run(&connection, "run_2", alfred, None, "done");
    connection
        .execute(
            "INSERT INTO delegation (id, company_id, conversation_id, from_agent_id, to_agent_id,
                task, digest, state, depth, turn_key, run_id, result_message_id, created_at,
                updated_at)
             VALUES ('dlg_1', ?1, ?2, ?3, ?3, 'Tarea', 'dg-1', 'done', 1, 'turn-1', 'run_2',
                'msg_1', ?4, ?4)",
            params![company, conversation, alfred, NOW],
        )
        .expect("delegation");
    connection
        .execute(
            "INSERT INTO timeline_card (id, conversation_id, delegation_id, seq, title, summary,
                status, agent_id, created_at, updated_at)
             VALUES ('card_1', ?1, 'dlg_1', 2, 'Tarea', 'Hecha', 'done', ?2, ?3, ?3)",
            params![conversation, alfred, NOW],
        )
        .expect("card");
    connection
        .execute(
            "INSERT INTO delegation_synthesis (delegation_id, conversation_id, result_message_id,
                state, run_id, created_at, updated_at)
             VALUES ('dlg_1', ?1, 'msg_1', 'done', 'run_2', ?2, ?2)",
            params![conversation, NOW],
        )
        .expect("synthesis");
    connection
        .execute(
            "INSERT INTO approval (id, company_id, conversation_id, delegation_id, kind, tool,
                summary, risk, action_digest, status, scope, expires_at, created_at, updated_at)
             VALUES ('apr_1', ?1, ?2, 'dlg_1', 'mail', 'mail.send', 'Enviar', 'low', 'digest-1',
                'approved', 'once', '2026-10-28T11:00:00.000Z', ?3, ?3)",
            params![company, conversation, NOW],
        )
        .expect("approval");
    connection
        .execute(
            "INSERT INTO approval_audit (approval_id, action_digest, device_id, decision, scope,
                decided_at)
             VALUES ('apr_1', 'digest-1', ?1, 'approve', 'once', ?2)",
            params![DEVICE, NOW],
        )
        .expect("approval audit");
    connection
        .execute(
            "INSERT INTO timeline_approval (approval_id, conversation_id, seq, status, summary,
                decider_kind, decider_id, created_at)
             VALUES ('apr_1', ?1, 3, 'approved', 'Enviar', 'device', ?2, ?3)",
            params![conversation, DEVICE, NOW],
        )
        .expect("timeline approval");
    connection
        .execute(
            "INSERT INTO timeline_mail_draft (id, conversation_id, draft_id, seq, account, to_json,
                subject, preview, created_at)
             VALUES ('mdc_1', ?1, 'drf_1', 4, 'cuenta', '[]', 'Asunto', 'Vista', ?2)",
            params![conversation, NOW],
        )
        .expect("draft card");
    connection
        .execute(
            "INSERT INTO timeline_mail_card (id, conversation_id, login_id, seq, account,
                user_code, verification_uri, created_at, expires_at)
             VALUES ('mlc_1', ?1, 'lgn_1', 5, 'cuenta', 'ABCD', 'https://example.test', ?2,
                '2026-10-28T11:00:00.000Z')",
            params![conversation, NOW],
        )
        .expect("mail card");
    connection
        .execute(
            "INSERT INTO calendar_event (id, title, start_utc, all_day, timezone, origin_kind,
                origin_id, conversation_id, created_at, updated_at, updated_by)
             VALUES ('ev_1', 'Llamada', '2026-10-29T09:30:00.000Z', 0, 'Europe/Madrid', 'alfred',
                'agt_1', ?1, ?2, ?2, 'alfred')",
            params![conversation, NOW],
        )
        .expect("calendar event");
    connection
        .execute(
            "INSERT INTO timeline_calendar_card (id, conversation_id, event_id, seq, created_at)
             VALUES ('cc_1', ?1, 'ev_1', 6, ?2)",
            params![conversation, NOW],
        )
        .expect("calendar card");
    connection
        .execute(
            "INSERT INTO hook_token (token_hash, seat_id, run_id, conversation_id, created_at)
             VALUES ('hash_1', ?1, 'run_1', ?2, ?3)",
            params![alfred, conversation, NOW],
        )
        .expect("hook token");
    connection
        .execute(
            "INSERT INTO note (id, subject, body, origin_kind, origin_id, conversation_id,
                message_id, created_at, updated_at, updated_by)
             VALUES ('nt_1', 'Recordatorio', 'Llamar al cliente', 'chat', 'msg_1', ?1, 'msg_1',
                ?2, ?2, 'alfred')",
            params![conversation, NOW],
        )
        .expect("note");
    let attachment = format!("att_{}", hn_core_chat::new_ulid());
    connection
        .execute(
            "INSERT INTO attachment (id, conversation_id, client_id, name, declared_size,
                declared_sha256, state, final_size, final_sha256, created_at)
             VALUES (?1, ?2, 'client-1', 'presupuesto.pdf', 3, ?3, 'ready', 3, ?3, ?4)",
            params![attachment, conversation, SHA, NOW],
        )
        .expect("attachment");
    AttachmentFiles::new(&fixture.dir.join("attachments"))
        .finish(conversation, &attachment, b"abc")
        .expect("file");
}

/// Another conversation with a turn, a note and a token: it must survive the delete.
pub fn populate_other(fixture: &Fixture, other: &str) {
    let connection = second_connection(&fixture.db);
    insert_message(&connection, other, "msg_other", 1);
    insert_run(
        &connection,
        "run_other",
        &fixture.alfred,
        Some("msg_other"),
        "done",
    );
    connection
        .execute(
            "INSERT INTO note (id, subject, body, origin_kind, origin_id, conversation_id,
                message_id, created_at, updated_at, updated_by)
             VALUES ('nt_other', NULL, 'Otra nota', 'chat', 'msg_other', ?1, 'msg_other', ?2,
                ?2, 'alfred')",
            params![other, NOW],
        )
        .expect("other note");
    connection
        .execute(
            "INSERT INTO hook_token (token_hash, seat_id, run_id, conversation_id, created_at)
             VALUES ('hash_other', ?1, 'run_other', ?2, ?3)",
            params![fixture.alfred, other, NOW],
        )
        .expect("other token");
}

pub fn conversation_folder(fixture: &Fixture, conversation: &str) -> PathBuf {
    fixture.dir.join("attachments").join(conversation)
}
