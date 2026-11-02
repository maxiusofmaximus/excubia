//! A calendar reminder against a real host (F5): the card always arrives in the chat; the
//! live `alert` frame arrives only when a `notify` rule names it, and a `mute` rule silences
//! it even beside a `notify` one. The frame names no agent, title or text.

mod harness;

use harness::TestHost;
use hn_core_auth::Permission;
use hn_core_chat::NewConversation;
use hn_core_test_support::SignedClient;
use serde_json::{json, Value};
use tokio::sync::broadcast::{error::TryRecvError, Receiver};

/// The `alert` frames received since the last call.
fn alerts(receiver: &mut Receiver<hn_core_state::SharedFrame>) -> Vec<Value> {
    let mut found = Vec::new();
    loop {
        match receiver.try_recv() {
            Ok(frame) if frame.event == "alert" => found.push(frame.body.clone()),
            Ok(_) => {}
            Err(TryRecvError::Empty | TryRecvError::Lagged(_)) => return found,
            Err(TryRecvError::Closed) => return found,
        }
    }
}

/// A reminder that is due at once: its start is the current minute, in UTC.
fn due_now(host: &TestHost) -> String {
    let now = host.chat.now_text();
    format!("{}Z", &now[..16])
}

async fn create_due_event(host: &TestHost, writer: &SignedClient, title: &str) {
    let start = due_now(host);
    let (status, body) = writer
        .request(
            "POST",
            "/v1/calendar/events",
            Some(
                serde_json::to_vec(&json!({
                    "title": title,
                    "start": start,
                    "reminder_minutes": 0,
                }))
                .expect("json"),
            ),
        )
        .await
        .expect("request");
    assert_eq!(status, 201, "{body}");
}

fn cards(host: &TestHost, conversation_id: &str) -> usize {
    host.chat
        .timeline(conversation_id, None, 100)
        .expect("timeline")
        .entries
        .iter()
        .filter(|entry| entry.kind == "card")
        .count()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_reminder_alerts_only_when_a_notify_rule_names_it_and_mute_wins() {
    let host = TestHost::start("alerts-reminders").await;
    host.chat
        .set_company_timezone("dev_harness", "Europe/Madrid")
        .expect("zone");
    // Pairing posts the notice first: a reminder without an origin goes to the newest
    // conversation, so «Trabajo» is created after it.
    let writer = host.paired_with("writer", &[Permission::Respond]).await;
    let conversation_id = host
        .chat
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id;
    let mut receiver = host.store.receiver();

    // No rule: the card arrives, and no alert frame.
    create_due_event(&host, &writer, "Sin regla").await;
    hn_core_host::calendar_reminders::deliver(&host.chat, &host.store);
    assert_eq!(cards(&host, &conversation_id), 1);
    assert!(alerts(&mut receiver).is_empty());

    // A notify rule: the card and one alert frame that names the rule.
    let (status, rule) = writer
        .request(
            "POST",
            "/v1/alerts/rules",
            Some(br#"{"trigger":"calendar_reminder","action":"notify"}"#.to_vec()),
        )
        .await
        .expect("request");
    assert_eq!(status, 201, "{rule}");
    let rule: Value = serde_json::from_str(&rule).expect("json");
    let rule_id = rule["rule"]["id"].as_str().expect("rule id").to_string();

    create_due_event(&host, &writer, "Con regla").await;
    hn_core_host::calendar_reminders::deliver(&host.chat, &host.store);
    assert_eq!(cards(&host, &conversation_id), 2);
    let frames = alerts(&mut receiver);
    assert_eq!(frames.len(), 1, "{frames:?}");
    assert_eq!(frames[0]["kind"], "calendar_reminder");
    assert_eq!(frames[0]["rule_id"], rule_id);
    assert_eq!(
        frames[0].as_object().map(|object| object.len()),
        Some(3),
        "the frame carries no agent, title or text: {}",
        frames[0]
    );

    // A mute rule beside the notify one: the card still arrives, the alert does not.
    let (status, _) = writer
        .request(
            "POST",
            "/v1/alerts/rules",
            Some(br#"{"trigger":"calendar_reminder","action":"mute"}"#.to_vec()),
        )
        .await
        .expect("request");
    assert_eq!(status, 201);

    create_due_event(&host, &writer, "Silenciado").await;
    hn_core_host::calendar_reminders::deliver(&host.chat, &host.store);
    assert_eq!(cards(&host, &conversation_id), 3);
    assert!(alerts(&mut receiver).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_burst_of_reminders_sends_every_card_but_only_a_few_frames() {
    let host = TestHost::start("alerts-reminders-burst").await;
    host.chat
        .set_company_timezone("dev_harness", "Europe/Madrid")
        .expect("zone");
    // Pairing posts the notice first: a reminder without an origin goes to the newest
    // conversation, so «Trabajo» is created after it.
    let writer = host.paired_with("writer", &[Permission::Respond]).await;
    let conversation_id = host
        .chat
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id;
    let (status, _) = writer
        .request(
            "POST",
            "/v1/alerts/rules",
            Some(br#"{"trigger":"calendar_reminder","action":"notify"}"#.to_vec()),
        )
        .await
        .expect("request");
    assert_eq!(status, 201);
    let mut receiver = host.store.receiver();

    for number in 0..5 {
        create_due_event(&host, &writer, &format!("Perdido {number}")).await;
    }
    hn_core_host::calendar_reminders::deliver(&host.chat, &host.store);
    assert_eq!(cards(&host, &conversation_id), 5);
    assert_eq!(alerts(&mut receiver).len(), 3);
}
