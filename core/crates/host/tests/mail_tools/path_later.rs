//! A file outside the workspace does not hold the turn: the tool answers
//! `pending_approval` at once, and the file joins the draft when the operator approves
//! the card, however late. A denial or a changed file attaches nothing.

use std::sync::Arc;

use excubia_chat::{ChatStore, Decision};
use excubia_seat::{AttachSource, MailAttach, MailCall};
use excubia_test_dir::TestDir;
use serde_json::{json, Value};

use super::mail_support::{ok, token_answer, Scripted};
use super::{actor_in, conversation, draft_to, say, sign_in, tools, OLD_REFRESH};

fn draft_answer() -> Value {
    json!({
        "id": "AAMk-borrador",
        "subject": "Bitácora",
        "toRecipients": [{"emailAddress": {"name": "Ana", "address": "ana@empresa.com"}}],
        "isDraft": true,
        "hasAttachments": false,
    })
}

fn alfred_said(chat: &ChatStore, conversation: &str) -> Vec<String> {
    chat.messages(conversation, None, None, 50)
        .expect("messages")
        .messages
        .into_iter()
        .filter(|message| message.author_kind == "agent")
        .map(|message| message.body_md)
        .collect()
}

/// A draft in a new conversation and a file outside the workspace; the tool's reply.
async fn asked(
    dir: &TestDir,
    chat: &Arc<ChatStore>,
    transport: &Arc<Scripted>,
) -> (excubia_host::mail_tools::MailTools, String, Value) {
    let (tools, store) = tools(dir, transport, chat, Some("app"));
    sign_in(&store, "trabajo", OLD_REFRESH);
    let conversation_id = conversation(chat);
    say(chat, &conversation_id, "escribe a ana@empresa.com");
    tools
        .call(&actor_in(&conversation_id), draft_to(&["ana@empresa.com"]))
        .await
        .expect("draft");
    let outside = dir.join("fuera");
    std::fs::create_dir_all(&outside).expect("dir");
    let file = outside.join("informe.txt");
    std::fs::write(&file, "contenido").expect("file");
    let reply = tools
        .call(
            &actor_in(&conversation_id),
            MailCall::AttachFile(MailAttach {
                account: "trabajo".into(),
                draft_id: "AAMk-borrador".into(),
                source: AttachSource::Path(file.display().to_string()),
            }),
        )
        .await
        .expect("answered at once");
    (tools, conversation_id, reply.result)
}

fn script() -> Arc<Scripted> {
    Arc::new(Scripted::new(vec![
        token_answer(),
        ok(draft_answer()),
        token_answer(),
        token_answer(),
        ok(json!({
            "@odata.type": "#microsoft.graph.fileAttachment",
            "id": "AAMk-adjunto",
            "name": "informe.txt",
            "size": 9,
        })),
    ]))
}

#[tokio::test]
async fn the_tool_answers_at_once_and_the_file_joins_the_draft_when_approved() {
    let dir = TestDir::new("mail-path-later-approve");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = script();
    let (tools, conversation_id, reply) = asked(&dir, &chat, &transport).await;

    assert_eq!(reply["status"], "pending_approval", "{reply}");
    let id = reply["approval_id"].as_str().expect("id").to_owned();
    assert!(
        !transport
            .posted()
            .iter()
            .any(|url| url.ends_with("/attachments")),
        "nothing uploaded before the answer"
    );
    let card = chat.approval(&id).expect("card");
    let payload = card.payload_redacted.as_ref().expect("payload");
    assert_eq!(payload["draft_id"], "AAMk-borrador");
    assert!(
        payload.get("account").is_none(),
        "a phone reads a card with an account as a mail send: {payload}"
    );

    let decided = chat
        .decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("approved");
    tools.attach_decided(&decided).await;

    assert!(transport
        .posted()
        .last()
        .expect("a post")
        .ends_with("/attachments"));
    let said = alfred_said(&chat, &conversation_id);
    assert!(
        said.iter()
            .any(|text| text.starts_with("Adjunté informe.txt")),
        "{said:?}"
    );
    // A second report of the same decision attaches nothing more.
    tools.attach_decided(&decided).await;
    assert_eq!(alfred_said(&chat, &conversation_id).len(), said.len());
}

#[tokio::test]
async fn a_denied_card_attaches_nothing_and_says_so() {
    let dir = TestDir::new("mail-path-later-deny");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = script();
    let (tools, conversation_id, reply) = asked(&dir, &chat, &transport).await;
    let id = reply["approval_id"].as_str().expect("id").to_owned();

    let decided = chat
        .decide_approval(&id, Decision::Deny, "device-1", "device-1")
        .expect("denied");
    tools.attach_decided(&decided).await;

    assert!(!transport
        .posted()
        .iter()
        .any(|url| url.ends_with("/attachments")));
    let said = alfred_said(&chat, &conversation_id);
    assert!(
        said.iter().any(|text| text.contains("lo denegaste")),
        "{said:?}"
    );
}

#[tokio::test]
async fn a_file_changed_before_the_answer_is_not_attached() {
    let dir = TestDir::new("mail-path-later-changed");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = script();
    let (tools, conversation_id, reply) = asked(&dir, &chat, &transport).await;
    let id = reply["approval_id"].as_str().expect("id").to_owned();
    std::fs::write(dir.join("fuera").join("informe.txt"), "otro contenido").expect("changed");

    let decided = chat
        .decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("approved");
    tools.attach_decided(&decided).await;

    assert!(!transport
        .posted()
        .iter()
        .any(|url| url.ends_with("/attachments")));
    let said = alfred_said(&chat, &conversation_id);
    assert!(
        said.iter()
            .any(|text| text.contains("mail_content_changed")),
        "{said:?}"
    );
}
