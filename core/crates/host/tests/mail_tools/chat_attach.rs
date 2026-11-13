//! A file the user sent in the chat, attached to a draft (DOC2): it goes under the
//! user's name without asking for an approval, and only from the conversation it was
//! sent in. Any other file is `attachment_not_found`, whatever the reason.

use std::sync::Arc;

use excubia_chat::{new_ulid, ChatStore, ReadyFile};
use excubia_seat::{AttachSource, MailAttach, MailCall};
use excubia_test_dir::TestDir;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::mail_support::{ok, token_answer, Scripted};
use super::{actor_in, conversation, draft_to, say, sign_in, tools, OLD_REFRESH};

const NAME: &str = "Informe final.xlsx";
const CONTENT_TEXT: &str = "contenido del informe";
const UNKNOWN: &str = "att_01JZ0000000000000000000000";

fn draft_answer() -> Value {
    json!({
        "id": "AAMk-borrador",
        "subject": "Presupuesto",
        "toRecipients": [{"emailAddress": {"name": "Ana", "address": "ana@empresa.com"}}],
        "isDraft": true,
        "hasAttachments": false,
    })
}

fn attachment_answer() -> Value {
    json!({
        "@odata.type": "#microsoft.graph.fileAttachment",
        "id": "AAMk-adjunto",
        "name": NAME,
        "size": CONTENT_TEXT.len(),
    })
}

fn sha256() -> String {
    format!("{:x}", Sha256::digest(CONTENT_TEXT))
}

/// A file the user is sending in `conversation_id`: stored, still uploading.
fn begin(chat: &ChatStore, conversation_id: &str) -> String {
    let attachment = chat
        .begin_attachment(
            conversation_id,
            &new_ulid(),
            NAME,
            CONTENT_TEXT.len() as u64,
            &sha256(),
        )
        .expect("begin");
    chat.attachment_files()
        .expect("files")
        .finish(conversation_id, &attachment.id, CONTENT_TEXT.as_bytes())
        .expect("stored");
    attachment.id
}

fn ready(chat: &ChatStore, attachment_id: &str) {
    let file = ReadyFile {
        mime: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into(),
        width: 0,
        height: 0,
        size: CONTENT_TEXT.len() as u64,
        sha256: sha256(),
    };
    chat.mark_ready(attachment_id, &file).expect("ready");
}

/// The message that sends the file: only then does it belong to the conversation.
fn send(chat: &ChatStore, conversation_id: &str, attachment_id: &str) {
    chat.append_user_message_with_attachments(
        conversation_id,
        &new_ulid(),
        "mira esto",
        None,
        &[attachment_id],
    )
    .expect("sent");
}

fn attach_chat(draft_id: &str, attachment_id: &str) -> MailCall {
    MailCall::AttachFile(MailAttach {
        account: "trabajo".into(),
        draft_id: draft_id.into(),
        source: AttachSource::Chat(attachment_id.into()),
    })
}

#[tokio::test]
async fn a_file_the_user_sent_is_attached_without_an_approval_and_audited_as_chat() {
    let dir = TestDir::new("mail-tools-chat-attach");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    chat.set_attachments_root(&dir.join("attachments"));
    let transport = Arc::new(Scripted::new(vec![
        token_answer(),
        ok(draft_answer()),
        token_answer(),
        ok(attachment_answer()),
    ]));
    let (tools, store) = tools(&dir, &transport, &chat, Some("app"));
    sign_in(&store, "trabajo", OLD_REFRESH);
    let conversation_id = conversation(&chat);
    say(&chat, &conversation_id, "escribe a ana@empresa.com");
    let draft = tools
        .call(&actor_in(&conversation_id), draft_to(&["ana@empresa.com"]))
        .await
        .expect("draft");
    let draft_id = draft.result["draft"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    let attachment_id = begin(&chat, &conversation_id);
    ready(&chat, &attachment_id);
    send(&chat, &conversation_id, &attachment_id);

    let reply = tools
        .call(
            &actor_in(&conversation_id),
            attach_chat(&draft_id, &attachment_id),
        )
        .await
        .expect("attached");

    assert_eq!(reply.result["attachment"]["id"], "AAMk-adjunto");
    assert_eq!(
        chat.pending_approval_count().expect("count"),
        0,
        "the chat was not asked for an approval"
    );
    assert!(transport
        .posted()
        .last()
        .expect("a post")
        .ends_with("/attachments"));
    let audit = std::fs::read_to_string(dir.join("data/mail/accounts/audit.jsonl")).expect("audit");
    assert!(
        audit
            .lines()
            .last()
            .expect("a line")
            .contains("\"decision\":\"chat\""),
        "{audit}"
    );
    assert!(!audit.contains(CONTENT_TEXT), "the audit keeps no content");
}

#[tokio::test]
async fn a_chat_file_that_is_foreign_not_ready_or_not_sent_is_not_found() {
    let dir = TestDir::new("mail-tools-chat-not-found");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    chat.set_attachments_root(&dir.join("attachments"));
    let answers = (0..4).map(|_| token_answer()).collect();
    let transport = Arc::new(Scripted::new(answers));
    let (tools, store) = tools(&dir, &transport, &chat, Some("app"));
    sign_in(&store, "trabajo", OLD_REFRESH);
    let conversation_id = conversation(&chat);
    let other_conversation = conversation(&chat);

    let foreign = begin(&chat, &other_conversation);
    ready(&chat, &foreign);
    send(&chat, &other_conversation, &foreign);
    let unsent = begin(&chat, &conversation_id);
    ready(&chat, &unsent);
    let uploading = begin(&chat, &conversation_id);

    for attachment_id in [foreign, unsent, uploading, UNKNOWN.to_string()] {
        let refused = tools
            .call(
                &actor_in(&conversation_id),
                attach_chat("AAMk-borrador", &attachment_id),
            )
            .await
            .expect_err("refused");
        assert_eq!(refused.code, "attachment_not_found", "{attachment_id}");
    }
    assert!(
        transport.posted().is_empty(),
        "no upload was started for any of them"
    );
}
