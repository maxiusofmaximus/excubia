//! The full signed flow against a real host: pair with a code, watch the
//! fake agent go `working → blocked → done`, send text and keys, read the
//! screen, mark seen, watch the pane leave the list, and end with a
//! revocation closing a live stream and refusing the next request.

mod harness;

use std::time::{Duration, Instant};

use harness::{wait_for_status, wait_until_gone, TestHost};
use hn_core_auth::Permission;

#[tokio::test(flavor = "multi_thread")]
async fn the_signed_flow_end_to_end() {
    let host = TestHost::start("signed-flow").await;
    let (client, paired) = host.pair("Escritorio test").await;
    // A new device is pending: it holds nothing until the host grants it (D3).
    assert!(paired.permissions.is_empty(), "{:?}", paired.permissions);
    assert!(paired.recovery_code.is_some(), "the code is shown once");
    assert_eq!(paired.owner_id.as_deref(), Some(""));
    let device_id = paired.device_id.clone();
    assert_eq!(client.state().await.expect_err("pending").status, 403);
    host.grant(&device_id, Permission::ReadState);

    // Reading is granted; acting still needs its own host-side grant.
    let forbidden = client
        .respond("w1:p1", "hola")
        .await
        .expect_err("no respond grant");
    assert_eq!(forbidden.status, 403);
    assert!(forbidden.body.contains("forbidden"), "{}", forbidden.body);
    host.grant(&device_id, Permission::Respond);
    host.grant(&device_id, Permission::SendKey);

    // Live stream for the whole flow.
    let stream_client = client.clone();
    let stream = tokio::spawn(async move { stream_client.events(8).await });

    let pane_id = host.spawn_fake(&["--delay-ms", "200"]).await;
    assert!(
        wait_for_status(&client, &pane_id, "working", Duration::from_secs(20)).await,
        "never saw working"
    );
    assert!(
        wait_for_status(&client, &pane_id, "blocked", Duration::from_secs(20)).await,
        "never saw blocked"
    );

    let response = client.respond(&pane_id, "y").await.expect("respond");
    assert_eq!(response["ok"], true);
    assert_eq!(response["result"]["pane_id"], pane_id.as_str());

    assert!(
        wait_for_status(&client, &pane_id, "done", Duration::from_secs(20)).await,
        "never saw done"
    );
    let read = client.read(&pane_id, 40).await.expect("read");
    assert_eq!(read["pane_id"], pane_id.as_str());
    let text = read["text"].as_str().unwrap_or_default();
    assert!(
        text.contains("RECIBIDO: y"),
        "sent text never arrived: {text}"
    );
    assert!(text.contains("terminado"), "agent never finished: {text}");

    client.seen(&pane_id).await.expect("seen");
    assert!(
        wait_for_status(&client, &pane_id, "idle", Duration::from_secs(20)).await,
        "seen did not clear the done badge"
    );

    // A second agent driven by keys, to cover the SEND_KEY path.
    let pane_two = host.spawn_fake(&["--delay-ms", "200"]).await;
    assert!(
        wait_for_status(&client, &pane_two, "blocked", Duration::from_secs(20)).await,
        "second agent never blocked"
    );
    client
        .keys(&pane_two, &["enter"], false)
        .await
        .expect("keys");
    assert!(
        wait_for_status(&client, &pane_two, "done", Duration::from_secs(20)).await,
        "keys did not approve the dialog"
    );

    // Leaving the prompt ends the process: the pane leaves the state list and
    // actions on it answer the contract's typed error.
    client.respond(&pane_id, "salir").await.expect("salir");
    assert!(
        wait_until_gone(&client, &pane_id, Duration::from_secs(20)).await,
        "the dead pane is still listed"
    );
    let gone = client
        .respond(&pane_id, "hola")
        .await
        .expect_err("dead pane");
    assert_eq!(gone.status, 400, "{}", gone.body);
    assert!(
        gone.body.contains("\"code\":\"pane_gone\""),
        "{}",
        gone.body
    );

    // The stream saw the transitions, in order and with monotonic revisions.
    let captured = stream.await.expect("stream task").expect("stream");
    assert!(captured.contains("event: state"), "opening frame missing");
    let mut revisions: Vec<u64> = Vec::new();
    for line in captured.lines().filter(|line| line.starts_with("data: ")) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line[6..]) {
            if let Some(revision) = value["revision"].as_u64() {
                revisions.push(revision);
            }
        }
    }
    revisions.dedup();
    assert!(revisions.len() >= 4, "frames: {revisions:?}");
    assert!(
        revisions.windows(2).all(|pair| pair[0] < pair[1]),
        "revisions must not rewind: {revisions:?}"
    );

    // Revocation: grant ADMIN, open a stream, revoke, and the stream closes
    // while the next request is refused with a uniform 401.
    host.grant(&device_id, Permission::Admin);
    let stream_client = client.clone();
    let started = Instant::now();
    let live = tokio::spawn(async move { stream_client.events(10).await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    client.revoke(&device_id).await.expect("revoke");
    let closed = live.await.expect("stream task").expect("stream");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the revoked stream must close promptly, took {:?}",
        started.elapsed()
    );
    assert!(
        closed.contains("event: state"),
        "the stream had connected: {closed}"
    );
    let refused = client.state().await.expect_err("revoked device");
    assert_eq!(refused.status, 401, "{}", refused.body);
    assert!(refused.body.contains("unauthorized"), "{}", refused.body);
}

#[tokio::test(flavor = "multi_thread")]
async fn state_revision_continues_after_a_restart() {
    let host = TestHost::start("restart").await;
    let (client, paired) = host.pair("restart test").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.spawn_fake(&["--delay-ms", "100"]).await;
    let revision_before = host.store.revision().await;
    assert!(revision_before >= 1);

    // A fresh store over the same counters file: what a host restart does.
    let reopened = hn_core_state::StateStore::open(
        "test",
        Some(hn_core_host::paths::counters_path(&host.data_dir)),
    );
    assert!(
        reopened.revision().await >= revision_before,
        "the revision must not rewind across a restart"
    );
    // The client keeps working against the live host.
    assert!(client.state().await.is_ok());
}
