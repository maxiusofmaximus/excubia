//! A pane whose agent has exited is dead: it must not appear in the state
//! list, and every action against it answers the contract's typed error
//! (`errors.md`), never a generic `409`.

mod harness;

use std::time::Duration;

use harness::{wait_for_status, wait_until_gone, TestHost};
use hn_core_auth::Permission;

#[tokio::test(flavor = "multi_thread")]
async fn a_dead_pane_leaves_the_list_and_answers_a_typed_error() {
    let host = TestHost::start("dead-pane").await;
    let (client, paired) = host.pair("dead pane test").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    host.grant(&paired.device_id, Permission::SendKey);

    // Watch the stream before the agent exists, so the created and closed
    // frames are both captured.
    let stream_client = client.clone();
    let stream = tokio::spawn(async move { stream_client.events(6).await });
    // The stream only sees frames published after it subscribes (its first
    // frame is a snapshot of what already exists). Give the subscription time
    // to land before the pane is created, or `pane.created` is missed.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // The quick scenario exits by itself after the busy phase.
    let pane_id = host
        .spawn_fake(&["--scenario", "quick", "--delay-ms", "150"])
        .await;
    assert!(
        wait_for_status(&client, &pane_id, "working", Duration::from_secs(20)).await,
        "never saw the agent working"
    );
    assert!(
        wait_until_gone(&client, &pane_id, Duration::from_secs(20)).await,
        "the dead pane is still listed"
    );

    let respond = client
        .respond(&pane_id, "hola")
        .await
        .expect_err("dead pane");
    assert_eq!(respond.status, 400, "{}", respond.body);
    assert!(
        respond.body.contains("\"code\":\"pane_gone\""),
        "{}",
        respond.body
    );

    let keys = client
        .keys(&pane_id, &["enter"], false)
        .await
        .expect_err("dead pane");
    assert_eq!(keys.status, 400, "{}", keys.body);
    assert!(
        keys.body.contains("\"code\":\"pane_gone\""),
        "{}",
        keys.body
    );

    let read = client.read(&pane_id, 40).await.expect_err("dead pane");
    assert_eq!(read.status, 400, "{}", read.body);
    assert!(
        read.body.contains("\"code\":\"pane_gone\""),
        "{}",
        read.body
    );

    // The stream learned about it without polling.
    let captured = stream.await.expect("stream task").expect("stream");
    assert!(
        captured.contains("event: pane.created"),
        "no pane.created frame: {captured}"
    );
    assert!(
        captured.contains("event: pane.closed"),
        "no pane.closed frame: {captured}"
    );
}
