//! Attachments in the user's message (ATT5): the files a message sends are linked
//! in the same transaction as the message, and the timeline and the agent's
//! context show them.

use excubia_core_chat::{
    new_ulid, AcceptedMessage, AttachmentError, AttachmentFiles, ChatError, ChatStore,
    NewConversation, ReadyFile, TimelineEntry,
};
use excubia_core_test_dir::TestDir;

fn store(name: &str) -> (ChatStore, TestDir) {
    let dir = TestDir::new(&format!("core-chat-attachment-messages-{name}"));
    let store = ChatStore::open(&dir.join("chat.db")).expect("open store");
    (store, dir)
}

fn conversation(store: &ChatStore) -> String {
    store
        .create_conversation(NewConversation {
            title: Some("Adjuntos".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

fn digest() -> String {
    "ab".repeat(32)
}

/// An upload that is `ready`: a 100-byte WebP of 800x600.
fn ready(store: &ChatStore, conversation_id: &str) -> String {
    let attachment = store
        .begin_attachment(conversation_id, &new_ulid(), "captura.png", 100, &digest())
        .expect("begin");
    let file = ReadyFile {
        mime: "image/webp".to_string(),
        width: 800,
        height: 600,
        size: 100,
        sha256: digest(),
    };
    store.mark_ready(&attachment.id, &file).expect("ready");
    attachment.id
}

fn send(
    store: &ChatStore,
    conversation_id: &str,
    client_id: &str,
    attachment_ids: &[&str],
) -> Result<AcceptedMessage, ChatError> {
    store.append_user_message_with_attachments(
        conversation_id,
        client_id,
        "mira esto",
        None,
        attachment_ids,
    )
}

fn refusal(result: Result<AcceptedMessage, ChatError>) -> AttachmentError {
    match result {
        Err(ChatError::Attachment(error)) => *error,
        other => panic!("expected an attachment refusal, got {other:?}"),
    }
}

fn timeline(store: &ChatStore, conversation_id: &str) -> Vec<TimelineEntry> {
    store
        .timeline(conversation_id, None, 50)
        .expect("timeline")
        .entries
}

fn message_count(store: &ChatStore, conversation_id: &str) -> usize {
    timeline(store, conversation_id)
        .iter()
        .filter(|entry| entry.kind == "message")
        .count()
}

#[test]
fn one_and_four_files_show_in_the_timeline_with_their_metadata() {
    let (store, _dir) = store("timeline");
    let conversation_id = conversation(&store);
    let one = ready(&store, &conversation_id);
    let four: Vec<String> = (0..4).map(|_| ready(&store, &conversation_id)).collect();
    let four_refs: Vec<&str> = four.iter().map(String::as_str).collect();

    let single = send(&store, &conversation_id, "c-1", &[&one]).expect("one file");
    let many = send(&store, &conversation_id, "c-4", &four_refs).expect("four files");

    let entries = timeline(&store, &conversation_id);
    let of = |message_id: &str| {
        entries
            .iter()
            .find(|entry| entry.message_id.as_deref() == Some(message_id))
            .expect("message in the timeline")
    };
    assert_eq!(of(&single.message.id).attachments.len(), 1);
    let files = &of(&many.message.id).attachments;
    assert_eq!(files.len(), 4);
    for id in &four {
        assert!(files.iter().any(|file| &file.id == id), "{id}");
    }
    let first = &files[0];
    assert_eq!(first.name, "captura.png");
    assert_eq!(first.mime, "image/webp");
    assert_eq!((first.size, first.width, first.height), (100, 800, 600));
    let json = serde_json::to_value(first).expect("json");
    assert_eq!(json["w"], 800);
    assert_eq!(json["h"], 600);
}

#[test]
fn a_message_without_files_keeps_the_timeline_shape() {
    let (store, _dir) = store("no-files");
    let conversation_id = conversation(&store);
    send(&store, &conversation_id, "c-0", &[]).expect("no files");
    let entry = &timeline(&store, &conversation_id)[0];
    assert!(entry.attachments.is_empty());
    let json = serde_json::to_value(entry).expect("json");
    assert!(json.get("attachments").is_none());
}

#[test]
fn five_files_refuse_the_message_and_leave_nothing() {
    let (store, _dir) = store("five");
    let conversation_id = conversation(&store);
    let five: Vec<String> = (0..5).map(|_| ready(&store, &conversation_id)).collect();
    let refs: Vec<&str> = five.iter().map(String::as_str).collect();

    let error = refusal(send(&store, &conversation_id, "c-5", &refs));

    assert!(matches!(error, AttachmentError::TooManyAttachments));
    assert_eq!(message_count(&store, &conversation_id), 0);
    assert_eq!(store.attachment(&five[0]).expect("row").message_id, None);
}

#[test]
fn a_file_of_another_conversation_refuses_the_whole_message() {
    let (store, _dir) = store("foreign");
    let conversation_id = conversation(&store);
    let other = conversation(&store);
    let own = ready(&store, &conversation_id);
    let foreign = ready(&store, &other);

    let error = refusal(send(&store, &conversation_id, "c-x", &[&own, &foreign]));

    assert!(matches!(error, AttachmentError::UnknownAttachment));
    assert_eq!(message_count(&store, &conversation_id), 0);
    assert_eq!(store.attachment(&own).expect("row").message_id, None);
}

#[test]
fn a_file_that_is_not_ready_refuses_the_message() {
    let (store, _dir) = store("not-ready");
    let conversation_id = conversation(&store);
    let uploading = store
        .begin_attachment(&conversation_id, &new_ulid(), "a.png", 1, &digest())
        .expect("begin")
        .id;

    let error = refusal(send(&store, &conversation_id, "c-n", &[&uploading]));

    assert!(matches!(error, AttachmentError::NotReady));
    assert_eq!(message_count(&store, &conversation_id), 0);
}

#[test]
fn a_file_already_sent_cannot_go_in_a_second_message() {
    let (store, _dir) = store("sent-twice");
    let conversation_id = conversation(&store);
    let file = ready(&store, &conversation_id);
    send(&store, &conversation_id, "c-1", &[&file]).expect("first");

    let error = refusal(send(&store, &conversation_id, "c-2", &[&file]));

    assert!(matches!(error, AttachmentError::AlreadyLinked));
    assert_eq!(message_count(&store, &conversation_id), 1);
}

#[test]
fn a_retry_with_the_same_client_id_returns_the_first_message_and_links_nothing() {
    let (store, _dir) = store("retry");
    let conversation_id = conversation(&store);
    let first = ready(&store, &conversation_id);
    let second = ready(&store, &conversation_id);
    let original = send(&store, &conversation_id, "c-1", &[&first]).expect("first");

    let retry = send(&store, &conversation_id, "c-1", &[&second]).expect("retry");

    assert!(retry.duplicate);
    assert_eq!(retry.message.id, original.message.id);
    assert_eq!(store.attachment(&second).expect("row").message_id, None);
    assert_eq!(message_count(&store, &conversation_id), 1);
}

#[test]
fn the_context_carries_each_file_with_its_host_path() {
    let (store, dir) = store("context");
    let root = dir.join("attachments");
    store.set_attachments_root(&root);
    let conversation_id = conversation(&store);
    let file = ready(&store, &conversation_id);
    let message = send(&store, &conversation_id, "c-1", &[&file]).expect("send");

    let context = store
        .context(&conversation_id, 20, 24_000)
        .expect("context");

    let attached = &context.attachments[&message.message.id][0];
    assert_eq!(attached.file.id, file);
    let expected = AttachmentFiles::new(&root)
        .file_path(&conversation_id, &file)
        .expect("path");
    assert_eq!(attached.path, expected);
}
