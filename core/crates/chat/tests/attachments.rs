//! Attachments (ATT1): the uploads and their quotas, the link to a message, the
//! files that hold the bytes, and what survives a reopen.

use std::path::PathBuf;

use excubia_core_chat::{
    new_ulid, Attachment, AttachmentError, AttachmentFiles, AttachmentState, ChatError, ChatStore,
    NewConversation, ReadyFile, CHUNK_MAX_BYTES, CONVERSATION_QUOTA_BYTES, FILE_MAX_BYTES,
};
use excubia_core_test_dir::TestDir;

const MIB: u64 = 1024 * 1024;

fn store(name: &str) -> (ChatStore, PathBuf, TestDir) {
    let dir = TestDir::new(&format!("core-chat-attachments-{name}"));
    let path = dir.join("chat.db");
    let store = ChatStore::open(&path).expect("open store");
    (store, path, dir)
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

fn message(store: &ChatStore, conversation_id: &str) -> String {
    store
        .append_user_message(conversation_id, &new_ulid(), "mira esto", None)
        .expect("message")
        .message
        .id
}

fn digest() -> String {
    "ab".repeat(32)
}

fn upload(
    store: &ChatStore,
    conversation_id: &str,
    client_id: &str,
    size: u64,
) -> Result<Attachment, AttachmentError> {
    store.begin_attachment(conversation_id, client_id, "captura.png", size, &digest())
}

fn ready_file(size: u64) -> ReadyFile {
    ReadyFile {
        mime: "image/webp".to_string(),
        width: 800,
        height: 600,
        size,
        sha256: digest(),
    }
}

/// An upload that is already `ready` with a `size`-byte file.
fn ready(store: &ChatStore, conversation_id: &str, size: u64) -> String {
    let attachment = upload(store, conversation_id, &new_ulid(), size).expect("begin");
    store
        .mark_ready(&attachment.id, &ready_file(size))
        .expect("ready");
    attachment.id
}

fn files(dir: &TestDir) -> AttachmentFiles {
    AttachmentFiles::new(&dir.join("attachments"))
}

#[test]
fn begin_is_idempotent_by_client_id() {
    let (store, _, _dir) = store("idempotent");
    let conversation_id = conversation(&store);
    let first = upload(&store, &conversation_id, "client-1", 1024).expect("first");
    let again = upload(&store, &conversation_id, "client-1", 1024).expect("again");
    assert_eq!(first.id, again.id);
    assert_eq!(first.state, AttachmentState::Uploading);
    assert_eq!(first.declared_size, 1024);
    assert_eq!(first.name, "captura.png");
    assert_eq!(first.message_id, None);
}

#[test]
fn begin_refuses_bad_names_digests_and_sizes() {
    let (store, _, _dir) = store("begin-refusals");
    let conversation_id = conversation(&store);
    let multiline = store.begin_attachment(&conversation_id, "c", "a\nb", 1, &digest());
    assert!(matches!(multiline, Err(AttachmentError::InvalidName)));
    let short_digest = store.begin_attachment(&conversation_id, "c", "a.png", 1, "zz");
    assert!(matches!(short_digest, Err(AttachmentError::InvalidSha256)));
    let too_big = upload(&store, &conversation_id, "c", FILE_MAX_BYTES + 1);
    assert!(matches!(too_big, Err(AttachmentError::FileTooLarge)));
}

#[test]
fn begin_needs_an_open_conversation() {
    let (store, _, _dir) = store("begin-unknown");
    let unknown = upload(&store, "cnv_01J5Z8K2M3N4P5Q6R7S8T9V0WX", "c", 1);
    assert!(matches!(
        unknown,
        Err(AttachmentError::Chat(ChatError::NoSuchConversation))
    ));
}

#[test]
fn the_conversation_quota_refuses_the_eleventh_full_file() {
    let (store, _, _dir) = store("conversation-quota");
    let conversation_id = conversation(&store);
    for index in 0..10 {
        upload(&store, &conversation_id, &format!("c-{index}"), 10 * MIB).expect("within quota");
    }
    let refused = upload(&store, &conversation_id, "c-10", 10 * MIB);
    assert!(matches!(refused, Err(AttachmentError::ConversationQuota)));
    assert_eq!(CONVERSATION_QUOTA_BYTES, 100 * MIB);
}

#[test]
fn the_host_quota_counts_every_conversation() {
    let (store, _, _dir) = store("host-quota");
    for index in 0..10 {
        let conversation_id = conversation(&store);
        for file in 0..10 {
            upload(
                &store,
                &conversation_id,
                &format!("{index}-{file}"),
                10 * MIB,
            )
            .expect("full conversation");
        }
    }
    let other = conversation(&store);
    upload(&store, &other, "o-1", 10 * MIB).expect("1020 MiB");
    upload(&store, &other, "o-2", 10 * MIB).expect("1020 MiB");
    let refused = upload(&store, &other, "o-3", 10 * MIB);
    assert!(matches!(refused, Err(AttachmentError::HostQuota)));
}

#[test]
fn the_final_size_counts_when_a_file_becomes_ready() {
    let (store, _, _dir) = store("final-size");
    let conversation_id = conversation(&store);
    let small = upload(&store, &conversation_id, "small", 1).expect("declared 1 byte");
    for index in 0..9 {
        upload(&store, &conversation_id, &format!("full-{index}"), 10 * MIB).expect("10 MiB");
    }
    upload(&store, &conversation_id, "last", 10 * MIB - 1).expect("fills to 100 MiB");
    let refused = store.mark_ready(&small.id, &ready_file(10 * MIB));
    assert!(matches!(refused, Err(AttachmentError::ConversationQuota)));
    assert_eq!(
        store.attachment(&small.id).expect("row").state,
        AttachmentState::Uploading
    );
}

#[test]
fn links_are_all_or_nothing_and_stay_in_the_conversation() {
    let (store, _, _dir) = store("link");
    let conversation_id = conversation(&store);
    let other = conversation(&store);
    let first = ready(&store, &conversation_id, 100);
    let second = ready(&store, &conversation_id, 100);
    let foreign = ready(&store, &other, 100);
    let message_id = message(&store, &conversation_id);

    let mixed = store.link_to_message(&conversation_id, &message_id, &[&first, &foreign]);
    assert!(matches!(mixed, Err(AttachmentError::UnknownAttachment)));
    assert_eq!(store.attachment(&first).expect("row").message_id, None);

    store
        .link_to_message(&conversation_id, &message_id, &[&first, &second])
        .expect("both link");
    assert_eq!(
        store.attachment(&second).expect("row").message_id,
        Some(message_id.clone())
    );

    let again = message(&store, &conversation_id);
    let taken = store.link_to_message(&conversation_id, &again, &[&first]);
    assert!(matches!(taken, Err(AttachmentError::AlreadyLinked)));

    let other_message = message(&store, &other);
    let wrong_message = store.link_to_message(&conversation_id, &other_message, &[&foreign]);
    assert!(matches!(
        wrong_message,
        Err(AttachmentError::UnknownMessage)
    ));
}

#[test]
fn a_message_takes_at_most_four_ready_files() {
    let (store, _, _dir) = store("link-limits");
    let conversation_id = conversation(&store);
    let message_id = message(&store, &conversation_id);
    let five: Vec<String> = (0..5).map(|_| ready(&store, &conversation_id, 1)).collect();
    let refs: Vec<&str> = five.iter().map(String::as_str).collect();
    let too_many = store.link_to_message(&conversation_id, &message_id, &refs);
    assert!(matches!(too_many, Err(AttachmentError::TooManyAttachments)));

    let uploading = upload(&store, &conversation_id, "u", 1).expect("begin").id;
    let not_ready = store.link_to_message(&conversation_id, &message_id, &[&uploading]);
    assert!(matches!(not_ready, Err(AttachmentError::NotReady)));
}

#[test]
fn delete_unlinked_drops_only_the_files_no_message_takes() {
    let (store, _, _dir) = store("delete-unlinked");
    let conversation_id = conversation(&store);
    let message_id = message(&store, &conversation_id);
    let loose = ready(&store, &conversation_id, 1);
    let linked = ready(&store, &conversation_id, 1);
    store
        .link_to_message(&conversation_id, &message_id, &[&linked])
        .expect("link");

    let removed = store
        .delete_unlinked(&conversation_id, &[&loose, &linked])
        .expect("delete");
    assert_eq!(removed, vec![loose.clone()]);
    assert!(store.attachment(&loose).is_err());
    assert_eq!(store.attachment(&linked).expect("kept").id, linked);
}

#[test]
fn chunks_must_continue_the_stored_bytes() {
    let (store, _, dir) = store("offsets");
    let conversation_id = conversation(&store);
    let attachment = upload(&store, &conversation_id, "c", 6).expect("begin").id;
    let files = files(&dir);

    assert_eq!(
        files
            .append_chunk(&conversation_id, &attachment, 0, b"abc")
            .expect("first"),
        3
    );
    let repeated = files.append_chunk(&conversation_id, &attachment, 0, b"abc");
    assert!(matches!(
        repeated,
        Err(AttachmentError::OffsetMismatch { next_offset: 3 })
    ));
    let skipped = files.append_chunk(&conversation_id, &attachment, 5, b"x");
    assert!(matches!(
        skipped,
        Err(AttachmentError::OffsetMismatch { next_offset: 3 })
    ));
    assert_eq!(
        files
            .append_chunk(&conversation_id, &attachment, 3, b"def")
            .expect("next"),
        6
    );
}

/// Two chunks at the same offset: the second must see the first one's bytes.
#[test]
fn racing_chunks_at_the_same_offset_leave_one_winner() {
    let (store, _, dir) = store("race");
    let conversation_id = conversation(&store);
    let attachment = upload(&store, &conversation_id, "c", 6).expect("begin").id;
    let files = files(&dir);
    let racers: Vec<_> = (0..2)
        .map(|_| {
            let files = files.clone();
            let (conversation_id, attachment) = (conversation_id.clone(), attachment.clone());
            std::thread::spawn(move || {
                files
                    .append_chunk(&conversation_id, &attachment, 0, b"abc")
                    .is_ok()
            })
        })
        .collect();
    let winners = racers
        .into_iter()
        .map(|racer| racer.join().expect("thread"))
        .filter(|won| *won)
        .count();
    assert_eq!(winners, 1);
}

#[test]
fn chunk_and_file_caps_are_enforced() {
    let (store, _, dir) = store("caps");
    let conversation_id = conversation(&store);
    let attachment = upload(&store, &conversation_id, "c", FILE_MAX_BYTES)
        .expect("begin")
        .id;
    let files = files(&dir);
    let chunk = vec![0u8; CHUNK_MAX_BYTES];

    let oversized = vec![0u8; CHUNK_MAX_BYTES + 1];
    assert!(matches!(
        files.append_chunk(&conversation_id, &attachment, 0, &oversized),
        Err(AttachmentError::ChunkTooLarge)
    ));

    let mut offset = 0;
    while offset < FILE_MAX_BYTES {
        offset = files
            .append_chunk(&conversation_id, &attachment, offset, &chunk)
            .expect("within the file cap");
    }
    assert!(matches!(
        files.append_chunk(&conversation_id, &attachment, offset, b"x"),
        Err(AttachmentError::FileTooLarge)
    ));
}

#[test]
fn hostile_ids_never_reach_the_disk() {
    let (store, _, dir) = store("hostile");
    let conversation_id = conversation(&store);
    let files = files(&dir);
    for hostile in ["../x", "a/b", "C:\\x", "att_../../x"] {
        let refused = files.append_chunk(&conversation_id, hostile, 0, b"x");
        assert!(
            matches!(refused, Err(AttachmentError::InvalidId)),
            "{hostile}"
        );
    }
    let refused = files.append_chunk("..\\conv", "att_01J5Z8K2M3N4P5Q6R7S8T9V0WX", 0, b"x");
    assert!(matches!(refused, Err(AttachmentError::InvalidId)));
    assert!(!dir.join("attachments").exists(), "nothing was created");
}

#[test]
fn finish_stores_the_final_file_and_drops_the_part() {
    let (store, _, dir) = store("finish");
    let conversation_id = conversation(&store);
    let attachment = upload(&store, &conversation_id, "c", 3).expect("begin").id;
    let files = files(&dir);
    files
        .append_chunk(&conversation_id, &attachment, 0, b"abc")
        .expect("chunk");
    files
        .finish(&conversation_id, &attachment, b"final")
        .expect("finish");

    let folder = dir.join("attachments").join(&conversation_id);
    assert_eq!(
        std::fs::read(folder.join(&attachment)).expect("final file"),
        b"final"
    );
    assert!(!folder.join(format!("{attachment}.part")).exists());
    assert!(matches!(
        files.finish(
            &conversation_id,
            &attachment,
            &vec![0u8; FILE_MAX_BYTES as usize + 1]
        ),
        Err(AttachmentError::FileTooLarge)
    ));
}

#[test]
fn removing_a_conversation_removes_its_files() {
    let (store, _, dir) = store("remove");
    let conversation_id = conversation(&store);
    let attachment = upload(&store, &conversation_id, "c", 1).expect("begin").id;
    let files = files(&dir);
    files
        .finish(&conversation_id, &attachment, b"x")
        .expect("finish");
    files
        .remove_conversation(&conversation_id)
        .expect("remove conversation");
    assert!(!dir.join("attachments").join(&conversation_id).exists());
    files
        .remove_conversation(&conversation_id)
        .expect("removing twice is fine");
}

#[test]
fn the_store_knows_its_attachment_root_once_the_host_sets_it() {
    let (store, _, dir) = store("root");
    assert!(store.attachment_files().is_none());
    store.set_attachments_root(&dir.join("attachments"));
    assert!(store.attachment_files().is_some());
}

#[test]
fn a_reopened_store_keeps_the_uploads_and_their_quota() {
    let (store, path, _dir) = store("reopen");
    let conversation_id = conversation(&store);
    let kept = ready(&store, &conversation_id, 10 * MIB);
    drop(store);

    let reopened = ChatStore::open(&path).expect("reopen");
    let attachment = reopened.attachment(&kept).expect("kept");
    assert_eq!(attachment.state, AttachmentState::Ready);
    assert_eq!(attachment.size, Some(10 * MIB));
    assert_eq!(attachment.sha256.as_deref(), Some(digest().as_str()));
    for index in 0..9 {
        upload(
            &reopened,
            &conversation_id,
            &format!("after-{index}"),
            10 * MIB,
        )
        .expect("fills the quota with the kept file");
    }
    let refused = upload(&reopened, &conversation_id, &new_ulid(), 1);
    assert!(matches!(refused, Err(AttachmentError::ConversationQuota)));
}

const OTHER_ID: &str = "att_01J5Z8K2M3N4P5Q6R7S8T9V0WY";
const ID: &str = "att_01J5Z8K2M3N4P5Q6R7S8T9V0WX";
const CONVERSATION: &str = "cnv_01J5Z8K2M3N4P5Q6R7S8T9V0WX";

#[test]
fn a_preview_lives_beside_its_file_and_goes_with_it() {
    let dir = TestDir::new("core-chat-attachments-previews");
    let files = files(&dir);
    files
        .finish(CONVERSATION, ID, b"%PDF-1.4\n")
        .expect("store the document");
    assert_eq!(files.read_preview(CONVERSATION, ID, 1).expect("read"), None);

    files
        .write_preview(CONVERSATION, ID, 1, b"png one")
        .expect("write page 1");
    files
        .write_preview(CONVERSATION, ID, 2, b"png two")
        .expect("write page 2");
    files
        .write_preview(CONVERSATION, OTHER_ID, 1, b"other")
        .expect("write another file's preview");
    assert_eq!(
        files.read_preview(CONVERSATION, ID, 2).expect("read"),
        Some(b"png two".to_vec())
    );

    files.remove(CONVERSATION, ID).expect("remove the file");
    assert_eq!(files.read_preview(CONVERSATION, ID, 1).expect("read"), None);
    assert_eq!(files.read_preview(CONVERSATION, ID, 2).expect("read"), None);
    assert_eq!(
        files.read_preview(CONVERSATION, OTHER_ID, 1).expect("read"),
        Some(b"other".to_vec())
    );
}

#[test]
fn a_preview_takes_only_checked_ids() {
    let dir = TestDir::new("core-chat-attachments-preview-ids");
    let files = files(&dir);
    assert!(files.write_preview(CONVERSATION, "../x", 1, b"x").is_err());
    assert!(files.read_preview("../cnv", ID, 1).is_err());
    assert!(files.remove("../cnv", ID).is_err());
}

#[test]
fn only_excel_word_and_pdf_documents_are_previewable() {
    use excubia_core_chat::FileKind::{Document, Image};
    for name in ["Informe.xlsx", "carta.DOCX", "factura.pdf"] {
        assert!(excubia_core_chat::previewable(name, Document), "{name}");
    }
    for name in ["datos.csv", "notas.txt", "deck.pptx", "sin_extension"] {
        assert!(!excubia_core_chat::previewable(name, Document), "{name}");
    }
    assert!(!excubia_core_chat::previewable("foto.pdf", Image));
}
