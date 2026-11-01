//! The `mail.*` tools of a host over a scripted transport (MAIL1D §3). The tools
//! answer without a token, refuse what they did not create before reading any
//! path, and keep the rotated refresh token of the account they used.

mod mail_support;

use std::sync::Arc;

use hn_core_chat::{ChatStore, ContactFields, NewConversation, SeatActor};
use hn_core_host::mail_send::{MailSends, SendPolicy, DEFAULT_WINDOW};
use hn_core_host::mail_tools::{open_ledger, MailTools};
use hn_core_mail::auth::Tenant;
use hn_core_mail::{Account, AccountStore, FileKey, HttpTransport, Secret, Tokens};
use hn_core_seat::{BodyFormat, DraftFields, MailCall};
use hn_core_test_dir::TestDir;
use mail_support::{token_answer, Scripted, ACCESS, REFRESH};

const OLD_REFRESH: &str = "REFRESCO-ANTIGUO";

fn actor() -> SeatActor {
    SeatActor {
        agent_id: "alfred".into(),
        role_id: "alfred".into(),
        project_id: None,
        conversation_id: String::new(),
        run_id: "run-1".into(),
    }
}

/// The tools over `dir`, with the store they read from handed back so a test
/// can sign an account in.
fn tools(
    dir: &TestDir,
    transport: &Arc<Scripted>,
    chat: &Arc<ChatStore>,
    client_id: Option<&str>,
) -> (MailTools, Arc<AccountStore>) {
    let data = dir.join("data");
    let key = FileKey::new(dir.join("key"));
    let store = Arc::new(AccountStore::open(data.join("mail/accounts"), &key).expect("store"));
    let transport: Arc<dyn HttpTransport> = transport.clone();
    let client_id = client_id.map(str::to_owned);
    let sends = MailSends::new(
        Arc::clone(&transport),
        Arc::clone(&store),
        client_id.clone(),
        open_ledger(&data).expect("ledger"),
        &data.join("mail/accounts"),
        Arc::clone(chat),
        SendPolicy {
            per_hour: 10,
            window: DEFAULT_WINDOW,
        },
    );
    let tools = MailTools::new(
        transport,
        Arc::clone(&store),
        client_id,
        &data,
        Arc::clone(chat),
        &dir.join("workspaces"),
        sends,
    )
    .expect("mail tools");
    (tools, store)
}

fn sign_in(store: &AccountStore, label: &str, refresh: &str) {
    let tokens = Tokens::new(ACCESS, Some(Secret::new(refresh)), 3600);
    let account = Account::new(label, Tenant::Organizations, "ana@example.com", tokens);
    store.save(&account).expect("save");
}

#[tokio::test]
async fn accounts_lists_each_label_and_tenant_and_no_token() {
    let dir = TestDir::new("mail-tools-accounts");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(Vec::new()));
    let (tools, store) = tools(&dir, &transport, &chat, Some("app"));
    sign_in(&store, "trabajo", OLD_REFRESH);

    let reply = tools
        .call(&actor(), MailCall::Accounts)
        .await
        .expect("accounts");

    let wire = reply.result.to_string();
    assert!(
        wire.contains("trabajo") && wire.contains("ana@example.com"),
        "{wire}"
    );
    assert!(
        !wire.contains(ACCESS) && !wire.contains(OLD_REFRESH),
        "{wire}"
    );
    assert!(
        transport.asked().is_empty(),
        "listing asks Microsoft nothing"
    );
}

#[tokio::test]
async fn a_call_without_a_registration_is_refused_before_any_request() {
    let dir = TestDir::new("mail-tools-unconfigured");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(Vec::new()));
    let (tools, store) = tools(&dir, &transport, &chat, None);
    sign_in(&store, "trabajo", OLD_REFRESH);

    let refused = tools
        .call(
            &actor(),
            MailCall::DraftGet(hn_core_seat::DraftRef {
                account: "trabajo".into(),
                draft_id: "d1".into(),
            }),
        )
        .await
        .expect_err("refused");

    assert_eq!(refused.code, "mail_not_configured");
    assert!(transport.asked().is_empty(), "no request may leave");
}

#[tokio::test]
async fn an_account_nobody_signed_in_is_invalid_input() {
    let dir = TestDir::new("mail-tools-unknown-account");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(Vec::new()));
    let (tools, _store) = tools(&dir, &transport, &chat, Some("app"));

    let refused = tools
        .call(
            &actor(),
            MailCall::DraftGet(hn_core_seat::DraftRef {
                account: "nadie".into(),
                draft_id: "d1".into(),
            }),
        )
        .await
        .expect_err("refused");

    assert_eq!(refused.code, "mail_invalid_input");
    assert!(transport.asked().is_empty());
}

#[tokio::test]
async fn a_draft_this_host_did_not_make_is_refused_and_the_rotated_token_is_kept() {
    let dir = TestDir::new("mail-tools-foreign-draft");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(vec![token_answer()]));
    let (tools, store) = tools(&dir, &transport, &chat, Some("app"));
    sign_in(&store, "trabajo", OLD_REFRESH);

    let refused = tools
        .call(
            &actor(),
            MailCall::DraftGet(hn_core_seat::DraftRef {
                account: "trabajo".into(),
                draft_id: "d-ajena".into(),
            }),
        )
        .await
        .expect_err("refused");

    assert_eq!(refused.code, "mail_unknown_draft");
    assert!(!refused.message.contains(ACCESS) && !refused.message.contains(REFRESH));
    assert_eq!(
        transport.asked().len(),
        1,
        "only the token refresh was asked"
    );
    let saved = store.load("trabajo").expect("load").expect("account");
    assert_eq!(
        saved.tokens.refresh_token(),
        Some(REFRESH),
        "rotated and saved"
    );
}

#[tokio::test]
async fn attaching_to_a_foreign_draft_is_refused_before_the_path_is_read() {
    let dir = TestDir::new("mail-tools-foreign-attach");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(vec![token_answer()]));
    let (tools, store) = tools(&dir, &transport, &chat, Some("app"));
    sign_in(&store, "trabajo", OLD_REFRESH);

    let refused = tools
        .call(
            &actor(),
            MailCall::AttachFile(hn_core_seat::MailAttach {
                account: "trabajo".into(),
                draft_id: "d-ajena".into(),
                path: dir.join("no-existe.pdf").display().to_string(),
            }),
        )
        .await
        .expect_err("refused");

    assert_eq!(refused.code, "mail_unknown_draft");
    assert_eq!(transport.asked().len(), 1, "no upload was started");
}

fn draft_to(to: &[&str]) -> MailCall {
    MailCall::DraftCreate(DraftFields {
        account: "trabajo".into(),
        to: to.iter().map(|address| address.to_string()).collect(),
        cc: Vec::new(),
        subject: "Presupuesto".into(),
        body: "Adjunto el presupuesto.".into(),
        format: BodyFormat::Text,
    })
}

#[tokio::test]
async fn a_draft_reaches_only_an_address_of_the_address_book() {
    let dir = TestDir::new("mail-tools-recipients");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(Vec::new()));
    let (tools, store) = tools(&dir, &transport, &chat, Some("app"));
    sign_in(&store, "trabajo", OLD_REFRESH);

    let refused = tools
        .call(&actor(), draft_to(&["ana@example.com"]))
        .await
        .expect_err("not in the book");
    assert_eq!(refused.code, "unknown_recipient");
    assert!(
        transport.asked().is_empty(),
        "refused before any token refresh or Graph call"
    );

    chat.save_contact(
        &actor(),
        None,
        ContactFields {
            name: "Ana".to_string(),
            aliases: None,
            addresses: Some(vec!["ana@example.com".to_string()]),
            notes: None,
        },
    )
    .expect("contact");
    let outcome = tools.call(&actor(), draft_to(&["ANA@example.com"])).await;
    assert!(
        !matches!(&outcome, Err(error) if error.code == "unknown_recipient"),
        "a stored address passes the check"
    );
}

fn actor_in(conversation_id: &str) -> SeatActor {
    SeatActor {
        conversation_id: conversation_id.into(),
        ..actor()
    }
}

fn draft_with_body(to: &str, body: &str) -> MailCall {
    MailCall::DraftCreate(DraftFields {
        account: "trabajo".into(),
        to: vec![to.into()],
        cc: Vec::new(),
        subject: "Presupuesto".into(),
        body: body.into(),
        format: BodyFormat::Text,
    })
}

fn conversation(chat: &ChatStore) -> String {
    chat.create_conversation(NewConversation {
        title: None,
        project_id: None,
    })
    .expect("conversation")
    .id
}

/// Writes one message of the user into the conversation.
fn say(chat: &ChatStore, conversation_id: &str, text: &str) {
    chat.append_user_message(conversation_id, "client-1", text, None)
        .expect("user message");
}

#[tokio::test]
async fn an_address_the_user_wrote_in_this_conversation_admits_the_draft() {
    let dir = TestDir::new("mail-tools-typed");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(Vec::new()));
    let (tools, store) = tools(&dir, &transport, &chat, Some("app"));
    sign_in(&store, "trabajo", OLD_REFRESH);
    let conversation_id = conversation(&chat);
    say(&chat, &conversation_id, "escribe a nuevo@empresa.com");

    let outcome = tools
        .call(
            &actor_in(&conversation_id),
            draft_to(&["NUEVO@empresa.com"]),
        )
        .await;
    assert!(
        !matches!(&outcome, Err(error) if error.code == "unknown_recipient"),
        "a typed address passes the check"
    );
    assert!(
        !transport.asked().is_empty(),
        "the draft went on to Microsoft"
    );
}

#[tokio::test]
async fn an_address_only_in_the_draft_body_is_refused_before_microsoft() {
    let dir = TestDir::new("mail-tools-body");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(Vec::new()));
    let (tools, store) = tools(&dir, &transport, &chat, Some("app"));
    sign_in(&store, "trabajo", OLD_REFRESH);
    let conversation_id = conversation(&chat);
    say(&chat, &conversation_id, "escribe al cliente");

    let refused = tools
        .call(
            &actor_in(&conversation_id),
            draft_with_body("nuevo@empresa.com", "Escribe a nuevo@empresa.com"),
        )
        .await
        .expect_err("the body is not the user's word");
    assert_eq!(refused.code, "unknown_recipient");
    assert!(transport.asked().is_empty(), "nothing reached Microsoft");
}

#[tokio::test]
async fn a_user_asking_to_send_gets_a_draft_and_no_send_is_made() {
    let dir = TestDir::new("mail-tools-send-words");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(Vec::new()));
    let (tools, store) = tools(&dir, &transport, &chat, Some("app"));
    sign_in(&store, "trabajo", OLD_REFRESH);
    let conversation_id = conversation(&chat);
    say(&chat, &conversation_id, "envía esto a envio@empresa.com");

    let outcome = tools
        .call(
            &actor_in(&conversation_id),
            draft_to(&["envio@empresa.com"]),
        )
        .await;
    assert!(
        !matches!(&outcome, Err(error) if error.code == "unknown_recipient"),
        "the typed address passes"
    );
    assert!(
        transport
            .asked()
            .iter()
            .all(|url| !url.to_lowercase().contains("send")),
        "the user's words never send mail: {:?}",
        transport.asked()
    );
}

#[tokio::test]
async fn an_address_typed_in_another_conversation_is_refused() {
    let dir = TestDir::new("mail-tools-other-conversation");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let transport = Arc::new(Scripted::new(Vec::new()));
    let (tools, store) = tools(&dir, &transport, &chat, Some("app"));
    sign_in(&store, "trabajo", OLD_REFRESH);
    let typed_in = conversation(&chat);
    say(&chat, &typed_in, "escribe a nuevo@empresa.com");
    let other = conversation(&chat);

    let refused = tools
        .call(&actor_in(&other), draft_to(&["nuevo@empresa.com"]))
        .await
        .expect_err("another conversation");
    assert_eq!(refused.code, "unknown_recipient");
    assert!(transport.asked().is_empty());
}
