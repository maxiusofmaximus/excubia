//! The sign-in card of a mail login on the host (MAIL1D §2): a login started for
//! a conversation shows its code there while it waits, the card goes however the
//! login ends, and an unknown conversation is refused before Microsoft is asked.

mod mail_support;

use std::sync::Arc;
use std::time::Duration;

use excubia_core_chat::{ChatStore, NewConversation, TimelineEntry};
use excubia_core_host::mail_api::{LoginStatus, MailLogins};
use excubia_core_mail::auth::Tenant;
use excubia_core_mail::{AccountStore, FileKey, HttpResponse, HttpTransport, MailError};
use excubia_core_test_dir::TestDir;
use mail_support::{device_answer, ok, token_answer, Scripted, USER_CODE};
use serde_json::json;

fn service(dir: &TestDir, transport: &Arc<Scripted>, chat: &Arc<ChatStore>) -> MailLogins {
    let store =
        AccountStore::open(dir.join("accounts"), &FileKey::new(dir.join("key"))).expect("store");
    let transport: Arc<dyn HttpTransport> = transport.clone();
    MailLogins::new(
        transport,
        Arc::new(store),
        Some("app".to_owned()),
        Arc::clone(chat),
    )
}

fn new_conversation(chat: &ChatStore) -> String {
    chat.create_conversation(NewConversation {
        title: Some("Correo".to_owned()),
        project_id: None,
    })
    .expect("conversation")
    .id
}

fn cards(chat: &ChatStore, conversation_id: &str) -> Vec<TimelineEntry> {
    chat.timeline(conversation_id, None, 50)
        .expect("timeline")
        .entries
        .into_iter()
        .filter(|entry| entry.kind == "card")
        .collect()
}

/// Polls the login until it has an answer.
async fn settled(logins: &MailLogins, login_id: &str) -> LoginStatus {
    for _ in 0..500 {
        let status = logins.status(login_id).expect("a known login");
        if status != LoginStatus::Pending {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the login never settled");
}

#[tokio::test]
async fn the_card_shows_the_code_while_the_login_waits_and_goes_when_it_succeeds() {
    let dir = TestDir::new("mail-card-ok");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let conversation = new_conversation(&chat);
    let transport = Arc::new(Scripted::new(vec![
        device_answer(),
        token_answer(),
        ok(json!({"userPrincipalName": "ana@example.com"})),
    ]));
    let logins = service(&dir, &transport, &chat);

    // The card is written before the start returns, and the login task cannot
    // run until this test yields, so the card is still there to be read.
    let start = logins
        .start("trabajo", Tenant::Organizations, Some(&conversation))
        .await
        .expect("start");
    let shown = cards(&chat, &conversation);
    assert_eq!(shown.len(), 1, "one card while the login waits");
    assert_eq!(shown[0].user_code.as_deref(), Some(USER_CODE));
    let wire = serde_json::to_string(&shown[0]).expect("json");
    assert!(!wire.contains("device_code"), "{wire}");

    let status = settled(&logins, &start.login_id).await;

    assert!(matches!(status, LoginStatus::Ok { .. }), "{status:?}");
    assert!(cards(&chat, &conversation).is_empty());
}

#[tokio::test]
async fn the_card_goes_when_the_user_refuses_the_login() {
    let dir = TestDir::new("mail-card-denied");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let conversation = new_conversation(&chat);
    let transport = Arc::new(Scripted::new(vec![
        device_answer(),
        HttpResponse {
            status: 400,
            body: json!({"error": "access_denied"}).to_string(),
        },
    ]));
    let logins = service(&dir, &transport, &chat);
    let start = logins
        .start("trabajo", Tenant::Organizations, Some(&conversation))
        .await
        .expect("start");

    let status = settled(&logins, &start.login_id).await;

    assert_eq!(status, LoginStatus::Denied);
    assert!(cards(&chat, &conversation).is_empty());
}

#[tokio::test]
async fn an_unknown_conversation_is_refused_before_microsoft_is_asked() {
    let dir = TestDir::new("mail-card-unknown");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(vec![device_answer()]));
    let logins = service(&dir, &transport, &chat);

    let refused = logins
        .start("trabajo", Tenant::Organizations, Some("conv-missing"))
        .await
        .expect_err("refused");

    assert!(matches!(refused, MailError::InvalidInput(_)), "{refused:?}");
    assert!(transport.asked().is_empty(), "no device code may be asked");
}
