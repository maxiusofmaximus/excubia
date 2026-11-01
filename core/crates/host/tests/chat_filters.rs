//! The listing filters and the counts at HTTP level: the flag grammar (`1` or
//! `true`, anything else is `422`), the counts route, and the `READ_STATE`
//! grant they share with the listing.

mod harness;

use harness::TestHost;
use hn_core_auth::Permission;
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
    assert_eq!(counts["all"], 1);
    assert_eq!(counts["attention"], 0);
    assert_eq!(counts["working"], 0);
    assert_eq!(counts["archived"], 0);

    for name in ["attention", "working"] {
        let listed = client
            .chat_conversations(&[(name, "1")])
            .await
            .expect("filter");
        assert_eq!(listed["conversations"], Value::Array(Vec::new()), "{name}");
        let listed = client
            .chat_conversations(&[(name, "true")])
            .await
            .expect("filter");
        assert_eq!(listed["conversations"], Value::Array(Vec::new()), "{name}");

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
