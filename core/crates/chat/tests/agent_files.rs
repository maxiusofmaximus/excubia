//! Files Alfred creates (DOC8): a file is stored as a ready attachment with no
//! message, `chat.reply` links the ones it names, and a turn that names none still
//! leaves its files in a short message of Alfred.

use excubia_core_chat::{
    new_ulid, AcceptedMessage, AttachmentError, AttachmentFiles, ChatError, ChatStore,
    NewConversation, ReadyFile, TimelineEntry,
};
use excubia_core_test_dir::TestDir;

fn store(name: &str) -> (ChatStore, TestDir) {
    let dir = TestDir::new(&format!("core-chat-agent-files-{name}"));
    let store = ChatStore::open(&dir.join("chat.db")).expect("open store");
    store.set_attachments_root(&dir.join("attachments"));
    (store, dir)
}

fn conversation(store: &ChatStore, title: &str) -> String {
    store
        .create_conversation(NewConversation {
            title: Some(title.to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

/// A user message that opens a turn; the turn is started, as the host does.
fn open_turn(store: &ChatStore, conversation_id: &str, client_id: &str) -> AcceptedMessage {
    let accepted = store
        .append_user_message(conversation_id, client_id, "hazlo", None)
        .expect("message");
    assert!(store.start_run(&accepted.turn.id).expect("start"));
    accepted
}

fn created(store: &ChatStore, conversation_id: &str, run_id: &str, name: &str) -> String {
    store
        .create_agent_file(conversation_id, run_id, name, "text/csv", b"a,b\n1,2")
        .expect("created file")
        .id
}

fn timeline(store: &ChatStore, conversation_id: &str) -> Vec<TimelineEntry> {
    store
        .timeline(conversation_id, None, 50)
        .expect("timeline")
        .entries
}

fn agent_messages(store: &ChatStore, conversation_id: &str) -> Vec<TimelineEntry> {
    timeline(store, conversation_id)
        .into_iter()
        .filter(|entry| entry.kind == "message" && entry.author_kind.as_deref() == Some("agent"))
        .collect()
}

#[test]
fn a_created_file_is_a_ready_document_with_no_message() {
    let (store, dir) = store("created");
    let conversation_id = conversation(&store, "Excel");
    let turn = open_turn(&store, &conversation_id, "u-1");

    let file = store
        .create_agent_file(&conversation_id, &turn.turn.id, "Ventas.csv", "text/csv", b"a,b")
        .expect("created");

    assert_eq!(file.name, "Ventas.csv");
    assert_eq!(file.mime.as_deref(), Some("text/csv"));
    assert_eq!(file.size, Some(3));
    assert_eq!(file.message_id, None);
    let stored = AttachmentFiles::new(&dir.join("attachments"))
        .read_file(&conversation_id, &file.id)
        .expect("bytes kept as written");
    assert_eq!(stored, b"a,b");
}

#[test]
fn a_reply_that_names_the_file_shows_it_on_its_message() {
    let (store, _dir) = store("named");
    let conversation_id = conversation(&store, "Informe");
    let turn = open_turn(&store, &conversation_id, "u-1");
    let file = created(&store, &conversation_id, &turn.turn.id, "informe.docx");

    store
        .complete_run_with_files(&turn.turn.id, "Aquí tienes el informe.", &[], &[&file])
        .expect("reply");

    let replies = agent_messages(&store, &conversation_id);
    assert_eq!(replies.len(), 1, "the reply is the only agent message");
    assert_eq!(replies[0].attachments.len(), 1);
    assert_eq!(replies[0].attachments[0].id, file);
}

#[test]
fn a_turn_that_names_no_file_still_delivers_it_in_a_short_message() {
    let (store, _dir) = store("leftover");
    let conversation_id = conversation(&store, "Olvido");
    let turn = open_turn(&store, &conversation_id, "u-1");
    let file = created(&store, &conversation_id, &turn.turn.id, "Gastos.xlsx");

    store
        .complete_run_with_files(&turn.turn.id, "Listo.", &[], &[])
        .expect("reply");

    let messages = agent_messages(&store, &conversation_id);
    assert_eq!(messages.len(), 2, "the reply and the file's own message");
    assert_eq!(messages[1].preview.as_deref(), Some("Te dejo el archivo Gastos.xlsx"));
    assert_eq!(messages[1].attachments[0].id, file);
    assert_eq!(messages[1].attachments[0].name, "Gastos.xlsx");
    assert_eq!(store.attachment(&file).expect("row").message_id, messages[1].message_id.clone());
}

#[test]
fn a_file_of_another_conversation_refuses_the_reply_and_the_turn_stays_open() {
    let (store, _dir) = store("foreign");
    let here = conversation(&store, "Aquí");
    let other = conversation(&store, "Allí");
    let turn = open_turn(&store, &here, "u-1");
    let foreign = created(&store, &other, &turn.turn.id, "ajeno.csv");

    let refusal = store.complete_run_with_files(&turn.turn.id, "listo", &[], &[&foreign]);

    assert!(
        matches!(refusal, Err(ChatError::Attachment(ref error)) if matches!(**error, AttachmentError::UnknownAttachment)),
        "{refusal:?}"
    );
    assert!(agent_messages(&store, &here).is_empty());
    store
        .complete_run_with_reply(&turn.turn.id, "ahora sí", &[])
        .expect("the run is still open");
}

#[test]
fn a_file_already_in_a_message_cannot_be_named_again() {
    let (store, _dir) = store("linked");
    let conversation_id = conversation(&store, "Doble");
    let first = open_turn(&store, &conversation_id, "u-1");
    let file = created(&store, &conversation_id, &first.turn.id, "uno.csv");
    store
        .complete_run_with_files(&first.turn.id, "toma", &[], &[&file])
        .expect("first reply");
    let second = open_turn(&store, &conversation_id, "u-2");

    let refusal = store.complete_run_with_files(&second.turn.id, "otra vez", &[], &[&file]);

    assert!(
        matches!(refusal, Err(ChatError::Attachment(ref error)) if matches!(**error, AttachmentError::AlreadyLinked)),
        "{refusal:?}"
    );
}

#[test]
fn a_user_upload_cannot_be_sent_as_one_of_alfreds_files() {
    let (store, _dir) = store("upload");
    let conversation_id = conversation(&store, "Subida");
    let turn = open_turn(&store, &conversation_id, "u-1");
    let upload = store
        .begin_attachment(&conversation_id, &new_ulid(), "tuyo.csv", 3, &"ab".repeat(32))
        .expect("begin");
    store
        .mark_ready(
            &upload.id,
            &ReadyFile {
                mime: "text/csv".to_string(),
                width: 0,
                height: 0,
                size: 3,
                sha256: "ab".repeat(32),
            },
        )
        .expect("ready");

    let refusal = store.complete_run_with_files(&turn.turn.id, "listo", &[], &[&upload.id]);

    assert!(
        matches!(refusal, Err(ChatError::Attachment(ref error)) if matches!(**error, AttachmentError::UnknownAttachment)),
        "{refusal:?}"
    );
}

#[test]
fn a_file_past_the_conversation_quota_is_refused_and_leaves_no_row() {
    let (store, _dir) = store("quota");
    let conversation_id = conversation(&store, "Cuota");
    let turn = open_turn(&store, &conversation_id, "u-1");
    let ten_mebibytes = "ab".repeat(32);
    for number in 0..10 {
        store
            .begin_attachment(
                &conversation_id,
                &format!("big-{number}"),
                "grande.csv",
                10 * 1024 * 1024,
                &ten_mebibytes,
            )
            .expect("declared");
    }

    let refusal = store.create_agent_file(&conversation_id, &turn.turn.id, "mas.csv", "text/csv", b"x");

    assert!(
        matches!(refusal, Err(AttachmentError::ConversationQuota)),
        "{refusal:?}"
    );
}

#[test]
fn a_name_that_is_not_one_line_is_refused() {
    let (store, _dir) = store("name");
    let conversation_id = conversation(&store, "Nombre");
    let turn = open_turn(&store, &conversation_id, "u-1");

    let refusal = store.create_agent_file(&conversation_id, &turn.turn.id, "a\nb.csv", "text/csv", b"x");

    assert!(matches!(refusal, Err(AttachmentError::InvalidName)), "{refusal:?}");
}
