//! The pairing audit: each `POST /v1/pair` decision leaves one line, and no line
//! carries a code, a recovery code or a public key.

mod harness;

use harness::TestHost;
use excubia_core_chat::{ChatStore, ConversationFilter};
use excubia_core_host::pairing_audit::AUDIT_FILE;
use excubia_core_host::pairing_notice::NOTICE_TITLE;
use excubia_core_test_support::{pair_device, DemoIdentity};
use serde_json::Value;

#[tokio::test(flavor = "multi_thread")]
async fn a_pairing_and_a_refused_one_leave_two_lines_without_secrets() {
    let host = TestHost::start("pairing-audit").await;
    let code = host.pair_code();
    let phone = DemoIdentity::generate("", "Phone");
    let paired = pair_device(
        &host.base_url(),
        "p256",
        &phone.public_key_b64(),
        "Phone",
        &code,
    )
    .await
    .expect("pair");

    let stranger = DemoIdentity::generate("", "Intruso");
    let wrong_code = "ZZZZZZZ";
    let refused = pair_device(
        &host.base_url(),
        "p256",
        &stranger.public_key_b64(),
        "Intruso",
        wrong_code,
    )
    .await;
    assert!(refused.is_err(), "a wrong code is refused");

    let log = std::fs::read_to_string(host.data_dir.join(AUDIT_FILE)).expect("audit file");
    let lines: Vec<Value> = log
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect();
    assert_eq!(lines.len(), 2, "{log}");

    assert_eq!(lines[0]["outcome"], "paired");
    assert_eq!(lines[0]["via"], "code");
    assert_eq!(lines[0]["device_id"], paired.device_id.as_str());
    assert_eq!(lines[0]["label"], "Phone");

    assert_eq!(lines[1]["outcome"], "rejected");
    assert_eq!(lines[1]["via"], "code");
    assert!(lines[1]["device_id"].is_null());
    assert_eq!(lines[1]["label"], "Intruso");

    let recovery = paired
        .recovery_code
        .expect("the pairing returns its recovery code");
    for secret in [
        code.as_str(),
        recovery.as_str(),
        wrong_code,
        phone.public_key_b64().as_str(),
        stranger.public_key_b64().as_str(),
    ] {
        assert!(!log.contains(secret), "the audit leaked a secret");
    }
}

fn notice_conversations(chat: &ChatStore) -> Vec<String> {
    let filter = ConversationFilter {
        q: None,
        section_id: None,
        pinned: false,
        archived: false,
        attention: false,
        working: false,
        cursor: None,
        limit: 100,
    };
    chat.list_conversations(filter)
        .expect("list conversations")
        .conversations
        .into_iter()
        .filter(|conversation| conversation.title == NOTICE_TITLE)
        .map(|conversation| conversation.id)
        .collect()
}

fn notice_texts(chat: &ChatStore) -> Vec<String> {
    notice_conversations(chat)
        .iter()
        .flat_map(|id| {
            chat.messages(id, None, None, 100)
                .expect("messages")
                .messages
        })
        .map(|message| message.body_md)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pairing_leaves_one_notice_and_a_refusal_leaves_none() {
    let host = TestHost::start("pairing-notice").await;
    let phone = DemoIdentity::generate("", "Phone");
    pair_device(
        &host.base_url(),
        "p256",
        &phone.public_key_b64(),
        "Phone",
        &host.pair_code(),
    )
    .await
    .expect("pair");

    assert_eq!(notice_conversations(&host.chat).len(), 1);
    let texts = notice_texts(&host.chat);
    assert_eq!(texts.len(), 1, "{texts:?}");
    assert!(texts[0].starts_with("Nuevo dispositivo pendiente: `Phone`"));

    let stranger = DemoIdentity::generate("", "Intruso");
    let refused = pair_device(
        &host.base_url(),
        "p256",
        &stranger.public_key_b64(),
        "Intruso",
        "ZZZZZZZ",
    )
    .await;
    assert!(refused.is_err(), "a wrong code is refused");
    assert_eq!(notice_texts(&host.chat).len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_pairing_reuses_the_notice_conversation() {
    let host = TestHost::start("pairing-notice-reuse").await;
    for label in ["Phone", "Tablet"] {
        let device = DemoIdentity::generate("", label);
        pair_device(
            &host.base_url(),
            "p256",
            &device.public_key_b64(),
            label,
            &host.pair_code(),
        )
        .await
        .expect("pair");
    }

    assert_eq!(notice_conversations(&host.chat).len(), 1);
    assert_eq!(notice_texts(&host.chat).len(), 2);
}
