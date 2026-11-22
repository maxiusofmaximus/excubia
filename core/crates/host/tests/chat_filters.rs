//! The listing filters and the counts at HTTP level: the flag grammar (`1` or
//! `true`, anything else is `422`), the counts route, and the `READ_STATE`
//! grant they share with the listing.

mod harness;

use excubia_auth::Permission;
use harness::TestHost;
use serde_json::Value;

#[tokio::test(flavor = "multi_thread")]
async fn the_filters_and_counts_follow_the_contract() {
    let host = TestHost::start("chat-filters").await;
    let client = host
        .paired_with("Desktop chat", &[Permission::Respond])
        .await;
    client
        .chat_create("Plan semanal", None)
        .await
        .expect("create");

    let counts = client.chat_counts().await.expect("counts");
    assert_eq!(counts["ok"], true);
    // The pairing's notice and the plan; the notice is unread, so it alone waits.
    assert_eq!(counts["all"], 2);
    assert_eq!(counts["attention"], 1);
    assert_eq!(counts["working"], 0);
    assert_eq!(counts["archived"], 0);

    for name in ["attention", "working"] {
        let waiting = usize::from(name == "attention");
        for value in ["1", "true"] {
            let listed = client
                .chat_conversations(&[(name, value)])
                .await
                .expect("filter");
            assert_eq!(
                listed["conversations"].as_array().expect("rows").len(),
                waiting,
                "{name}={value}"
            );
        }

        for refused in ["yes", "0", "false", ""] {
            let error = client
                .chat_conversations(&[(name, refused)])
                .await
                .expect_err("refused flag");
            assert_eq!(error.status, 422, "{name}={refused}");
            let detail: Value = serde_json::from_str(&error.body).expect("json error");
            assert_eq!(detail["detail"]["field"], name);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_counts_and_filters_need_read_state() {
    let host = TestHost::start("chat-filters-permission").await;
    let (stranger, _) = host.pair("Sin permiso").await;

    let refused = stranger.chat_counts().await.expect_err("counts");
    assert_eq!(refused.status, 403);
    let refused = stranger
        .chat_conversations(&[("attention", "1")])
        .await
        .expect_err("filter");
    assert_eq!(refused.status, 403);
}
