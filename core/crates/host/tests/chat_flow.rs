//! The persistent chat at HTTP level: full turn flow with a fake seat,
//! pagination, search, metadata across a restart, contract errors and SSE
//! resumption by `Last-Event-ID`.

mod harness;

use std::time::Duration;

use harness::{SeatKind, TestHost};
use hn_core_auth::Permission;
use hn_core_chat::RunState;
use hn_core_test_support::SignedClient;
use serde_json::Value;
use tokio::time::{sleep, Instant};

const TTL: Duration = Duration::from_secs(45);

async fn wait_for_entries(
    client: &SignedClient,
    conversation_id: &str,
    want: usize,
    timeout: Duration,
) -> Value {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(timeline) = client
            .chat_timeline(conversation_id, &[("limit", "100")])
            .await
        {
            if timeline["entries"]
                .as_array()
                .is_some_and(|entries| entries.len() == want)
            {
                return timeline;
            }
        }
        assert!(
            Instant::now() < deadline,
            "timeline never reached {want} entries"
        );
        sleep(Duration::from_millis(50)).await;
    }
}

fn error_detail(body: &str) -> Value {
    let parsed: Value = serde_json::from_str(body).expect("json error body");
    parsed["detail"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_chat_flow_survives_a_restart_and_resumes_sse() {
    let host = TestHost::start("chat-flow").await;
    let (client, paired) = host.pair("Desktop chat").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);

    let bootstrap = client.chat_bootstrap().await.expect("bootstrap");
    assert_eq!(bootstrap["ok"], true);
    assert_eq!(bootstrap["company"]["name"], "Mi empresa");
    assert_eq!(bootstrap["alfred"]["name"], "Alfred");
    assert!(bootstrap["projects"]
        .as_array()
        .expect("projects")
        .is_empty());

    let created = client
        .chat_create("Plan semanal", None)
        .await
        .expect("create");
    let conversation_id = created["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    let detail = client
        .chat_conversation(&conversation_id)
        .await
        .expect("detail");
    assert_eq!(detail["participants"][0]["name"], "Alfred");

    let mut first_message = String::new();
    let mut first_turn = String::new();
    for (index, text) in ["uno", "dos", "tres"].iter().enumerate() {
        let response = client
            .chat_send(&conversation_id, &format!("client-{index}"), text)
            .await
            .expect("send");
        assert_eq!(response["turn"]["state"], "queued");
        assert_eq!(response["message"]["author_kind"], "user");
        if index == 0 {
            first_message = response["message"]["id"].as_str().expect("id").to_string();
            first_turn = response["turn"]["id"].as_str().expect("id").to_string();
        }
    }
    let timeline = wait_for_entries(&client, &conversation_id, 6, Duration::from_secs(10)).await;
    let entries = timeline["entries"].as_array().expect("entries");
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry["seq"].as_i64())
            .collect::<Vec<_>>(),
        (1_i64..=6).map(Some).collect::<Vec<_>>()
    );
    let authors = entries
        .iter()
        .map(|entry| entry["author_kind"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(authors.iter().filter(|kind| **kind == "user").count(), 3);
    assert_eq!(authors.iter().filter(|kind| **kind == "agent").count(), 3);
    let previews = entries
        .iter()
        .map(|entry| entry["preview"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>();
    for text in ["uno", "dos", "tres"] {
        assert!(previews.iter().any(|preview| preview == text), "{text}");
        assert!(
            previews
                .iter()
                .any(|preview| preview == &format!("Respuesta simulada: {text}")),
            "reply to {text}"
        );
    }

    let duplicate = client
        .chat_send(&conversation_id, "client-0", "uno")
        .await
        .expect("duplicate");
    assert_eq!(duplicate["message"]["id"], first_message.as_str());
    assert_eq!(duplicate["turn"]["id"], first_turn.as_str());
    assert_eq!(
        client
            .chat_timeline(&conversation_id, &[("limit", "100")])
            .await
            .expect("timeline")["entries"]
            .as_array()
            .expect("entries")
            .len(),
        6,
        "a repeated client_id is not stored twice"
    );

    let latest = client
        .chat_timeline(&conversation_id, &[("limit", "2")])
        .await
        .expect("page 1");
    assert_eq!(latest["entries"][0]["seq"], 5);
    assert_eq!(latest["next_before_seq"], 5);
    let older = client
        .chat_timeline(&conversation_id, &[("limit", "2"), ("before_seq", "5")])
        .await
        .expect("page 2");
    assert_eq!(older["entries"][0]["seq"], 3);
    let synced = client
        .chat_messages(&conversation_id, &[("after_seq", "4")])
        .await
        .expect("messages");
    assert_eq!(
        synced["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .map(|message| message["seq"].as_i64())
            .collect::<Vec<_>>(),
        vec![Some(5), Some(6)]
    );

    let found = client
        .chat_conversations(&[("q", "dos")])
        .await
        .expect("search");
    assert_eq!(found["conversations"].as_array().expect("rows").len(), 1);
    let by_title = client
        .chat_conversations(&[("q", "Plan")])
        .await
        .expect("title search");
    assert_eq!(by_title["conversations"][0]["id"], conversation_id.as_str());

    let section = client
        .chat_upsert_section(None, "Trabajo")
        .await
        .expect("section");
    let section_id = section["section"]["id"].as_str().expect("section id");
    client
        .chat_update(
            &conversation_id,
            serde_json::json!({"pinned": true, "section_id": section_id}),
        )
        .await
        .expect("pin and move");
    let read = client.chat_read(&conversation_id, 99).await.expect("read");
    assert_eq!(read["last_read_seq"], 6);
    assert_eq!(
        client.chat_bootstrap().await.expect("bootstrap")["unread"]["messages"],
        0
    );

    // SSE replay: everything after this revision is journaled and must come
    // back on a reconnect before any live frame.
    let last = host.chat.revision().expect("revision");
    client
        .chat_send(&conversation_id, "client-sse", "evento reanudado")
        .await
        .expect("send");
    sleep(Duration::from_millis(100)).await;
    let resumed = client
        .events_with_last_id(1, Some(last))
        .await
        .expect("resumed stream");
    assert!(
        resumed.contains(&format!("id: {}", last + 1)),
        "missing replay id in {resumed}"
    );
    assert!(resumed.contains("event: chat.message"), "{resumed}");
    assert!(resumed.contains("evento reanudado"), "{resumed}");

    // Live frames on the same stream.
    let live_client = client.clone();
    let live = tokio::spawn(async move { live_client.events(2).await });
    sleep(Duration::from_millis(300)).await;
    client
        .chat_send(&conversation_id, "client-live", "mensaje en vivo")
        .await
        .expect("live send");
    let live_text = live.await.expect("join").expect("stream");
    assert!(live_text.contains("event: chat.message"), "{live_text}");
    assert!(live_text.contains("mensaje en vivo"), "{live_text}");

    client
        .chat_update(&conversation_id, serde_json::json!({"archived": true}))
        .await
        .expect("archive");

    // Restart: history, order and metadata (including archived) are still there.
    let host = host.restart(TTL).await;
    let client = SignedClient::new(&host.base_url(), client.identity().clone());
    let detail = client
        .chat_conversation(&conversation_id)
        .await
        .expect("detail after restart");
    assert_eq!(detail["conversation"]["title"], "Plan semanal");
    assert_eq!(detail["conversation"]["pinned"], true);
    assert_eq!(detail["conversation"]["section_id"], section_id);
    assert_eq!(detail["conversation"]["archived"], true);
    let timeline = client
        .chat_timeline(&conversation_id, &[("limit", "100")])
        .await
        .expect("timeline after restart");
    assert_eq!(
        timeline["entries"].as_array().expect("entries").len(),
        10,
        "six initial entries plus two from each SSE send"
    );

    // Dedicated 422s carry code and field (CORE5 shape).
    let unknown_project = client
        .chat_create("Otro", Some("prj_nope"))
        .await
        .expect_err("unknown project");
    assert_eq!(unknown_project.status, 422);
    let body = error_detail(&unknown_project.body);
    assert_eq!(body["code"], "unknown_project");
    assert_eq!(body["field"], "project_id");

    let unknown_section = client
        .chat_update(
            &conversation_id,
            serde_json::json!({"section_id": "sec_nope"}),
        )
        .await
        .expect_err("unknown section");
    assert_eq!(unknown_section.status, 422);
    assert_eq!(error_detail(&unknown_section.body)["field"], "section_id");

    let long_text = "a".repeat(32 * 1024 + 1);
    let too_long = client
        .request(
            "POST",
            &format!("/v1/chat/conversations/{conversation_id}/messages"),
            Some(
                serde_json::to_vec(&serde_json::json!({"client_id": "long", "text": long_text}))
                    .expect("json"),
            ),
        )
        .await
        .expect("long message");
    assert_eq!(too_long.0, 422);
    let body = error_detail(&too_long.1);
    assert_eq!(body["code"], "invalid_body");
    assert_eq!(body["field"], "text");

    // An archived conversation still rejects new turns with the chat's own 409.
    let archived = client
        .chat_send(&conversation_id, "client-archived", "hola")
        .await
        .expect_err("archived");
    assert_eq!(archived.status, 409);
    assert_eq!(error_detail(&archived.body), "conversation_archived");
    let missing = client
        .chat_conversation("cnv_nope")
        .await
        .expect_err("missing");
    assert_eq!(missing.status, 404);
    assert_eq!(error_detail(&missing.body), "no_such_conversation");

    // A read-only device reads chat but cannot mutate it.
    let reader = host
        .paired_with("Phone reader", &[Permission::ReadState])
        .await;
    reader.chat_bootstrap().await.expect("read allowed");
    let forbidden = reader
        .request("POST", "/v1/chat/conversations", Some(b"{}".to_vec()))
        .await
        .expect("forbidden");
    assert_eq!(forbidden.0, 403, "{}", forbidden.1);
    assert_eq!(error_detail(&forbidden.1), "forbidden");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_queued_turn_is_interrupted_when_the_host_restarts() {
    let host = TestHost::start_with_seat("chat-interrupted", TTL, SeatKind::Silent).await;
    let (client, paired) = host.pair("Desktop chat").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    let conversation = client
        .chat_create("Turno largo", None)
        .await
        .expect("create")["conversation"]["id"]
        .as_str()
        .expect("id")
        .to_string();
    let accepted = client
        .chat_send(&conversation, "client-run", "sigue trabajando")
        .await
        .expect("send");
    let run_id = accepted["turn"]["id"].as_str().expect("run id").to_string();
    assert_eq!(accepted["turn"]["state"], "queued");
    assert_eq!(
        host.chat.run_state(&run_id).expect("run state"),
        Some(RunState::Queued)
    );

    let host = host.restart_with(TTL, SeatKind::Silent).await;
    assert_eq!(
        host.chat.run_state(&run_id).expect("run state"),
        Some(RunState::Interrupted)
    );
    let client = SignedClient::new(&host.base_url(), client.identity().clone());
    let timeline = client
        .chat_timeline(&conversation, &[("limit", "10")])
        .await
        .expect("timeline");
    assert_eq!(timeline["entries"].as_array().expect("entries").len(), 1);
}
