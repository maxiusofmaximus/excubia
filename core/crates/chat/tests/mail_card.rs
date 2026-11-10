//! The sign-in card of a mail login (MAIL1D §2): it shows the user code and the
//! page to type it on while the login waits, it goes when the login ends, and
//! it never carries a device code or a token.

use excubia_core_chat::{ChatStore, NewConversation, NewMailCard, TimelineEntry};

const CODE: &str = "ABCD-1234";
const PAGE: &str = "https://microsoft.com/devicelogin";

fn store_with_conversation() -> (ChatStore, String) {
    let store = ChatStore::in_memory().expect("store");
    let conversation = store
        .create_conversation(NewConversation {
            title: Some("Correo".to_string()),
            project_id: None,
        })
        .expect("conversation");
    (store, conversation.id)
}

fn show(store: &ChatStore, conversation_id: &str, login_id: &str) {
    store
        .open_mail_card(NewMailCard {
            conversation_id,
            login_id,
            account: "trabajo",
            user_code: CODE,
            verification_uri: PAGE,
            expires_in: 900,
        })
        .expect("card");
}

fn cards(store: &ChatStore, conversation_id: &str) -> Vec<TimelineEntry> {
    store
        .timeline(conversation_id, None, 50)
        .expect("timeline")
        .entries
        .into_iter()
        .filter(|entry| entry.kind == "card")
        .collect()
}

#[test]
fn a_sign_in_card_shows_the_code_and_the_page_and_no_token() {
    let (store, conversation) = store_with_conversation();

    show(&store, &conversation, "login-1");

    let shown = cards(&store, &conversation);
    assert_eq!(shown.len(), 1);
    let card = &shown[0];
    assert_eq!(card.title.as_deref(), Some("Conecta tu correo"));
    assert_eq!(card.account.as_deref(), Some("trabajo"));
    assert_eq!(card.user_code.as_deref(), Some(CODE));
    assert_eq!(card.verification_uri.as_deref(), Some(PAGE));
    let wire = serde_json::to_string(card).expect("json");
    assert!(!wire.contains("device_code"), "{wire}");
    assert!(!wire.contains("token"), "{wire}");
}

#[test]
fn a_sign_in_card_has_the_contract_fields_and_nothing_else_about_the_login() {
    let (store, conversation) = store_with_conversation();
    show(&store, &conversation, "login-1");

    let shown = cards(&store, &conversation);
    let wire = serde_json::to_value(&shown[0]).expect("json");
    assert_eq!(wire["kind"], "card");
    assert_eq!(wire["state"], "pending");
    assert_eq!(wire["account"], "trabajo");
    assert_eq!(wire["user_code"], CODE);
    assert_eq!(wire["verification_uri"], PAGE);
    let expires_at = wire["expires_at"].as_str().expect("expires_at");
    assert_eq!(
        expires_at.len(),
        "2026-10-25T10:12:00.000Z".len(),
        "{expires_at}"
    );
    assert!(expires_at.ends_with('Z'), "{expires_at}");
    assert!(wire.get("summary").is_none(), "{wire}");
    assert!(wire.get("status").is_none(), "{wire}");
}

#[test]
fn the_card_goes_when_its_login_ends() {
    let (store, conversation) = store_with_conversation();
    show(&store, &conversation, "login-1");

    store.close_mail_card("login-1").expect("close");

    assert!(cards(&store, &conversation).is_empty());
}

#[test]
fn closing_a_login_without_a_card_is_a_no_op() {
    let (store, conversation) = store_with_conversation();
    show(&store, &conversation, "login-1");

    store.close_mail_card("never-shown").expect("no-op");

    assert_eq!(cards(&store, &conversation).len(), 1);
}

#[test]
fn each_card_takes_its_own_place_in_the_conversation_order() {
    let (store, conversation) = store_with_conversation();
    show(&store, &conversation, "login-1");
    show(&store, &conversation, "login-2");

    let shown = cards(&store, &conversation);
    let seqs: Vec<i64> = shown.iter().map(|card| card.seq).collect();
    assert_eq!(shown.len(), 2);
    assert_ne!(seqs[0], seqs[1], "{seqs:?}");

    store.close_mail_card("login-1").expect("close");
    let left = cards(&store, &conversation);
    let newest = seqs.iter().max().copied().expect("two cards");
    assert_eq!(left.len(), 1);
    assert_eq!(
        left[0].seq, newest,
        "the second login's card is the one left"
    );
}

#[test]
fn a_card_for_an_unknown_conversation_is_refused() {
    let store = ChatStore::in_memory().expect("store");

    let refused = store
        .open_mail_card(NewMailCard {
            conversation_id: "conv-missing",
            login_id: "login-1",
            account: "trabajo",
            user_code: CODE,
            verification_uri: PAGE,
            expires_in: 900,
        })
        .expect_err("refused");

    assert!(
        matches!(refused, excubia_core_chat::ChatError::NoSuchConversation),
        "{refused:?}"
    );
}

#[test]
fn a_restart_takes_every_card_away() {
    let (store, conversation) = store_with_conversation();
    show(&store, &conversation, "login-1");
    show(&store, &conversation, "login-2");

    store.close_all_mail_cards().expect("close all");

    assert!(cards(&store, &conversation).is_empty());
}
