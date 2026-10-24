//! `GET /v1/catalog` and `POST /v1/agents/{pane_id}/stop`.

mod harness;

use std::time::Duration;

use harness::{process_exists, wait_for_status, wait_until_gone, TestHost};
use hn_core_auth::Permission;
use serde_json::{json, Value};

fn workdir() -> String {
    env!("CARGO_MANIFEST_DIR").to_string()
}

fn read_json(path: &std::path::Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("json")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_catalog_lists_names_and_availability_without_paths() {
    let host = TestHost::start("catalog").await;
    let client = host.paired_with("catalog reader", &[]).await;

    let catalog = client.catalog().await.expect("catalog");
    let agents = catalog["agents"].as_array().expect("agents");
    let fake = agents
        .iter()
        .find(|agent| agent["name"] == "fake")
        .expect("fake");
    assert_eq!(fake["available"], true);
    assert_eq!(fake["label"], "Fake agent");
    let missing = agents
        .iter()
        .find(|agent| agent["name"] == "missing")
        .expect("missing");
    assert_eq!(missing["available"], false);
    assert!(missing.get("label").is_none(), "{missing}");

    // Names and availability only: never a command, its arguments or a path.
    let raw = serde_json::to_string(&catalog).expect("json");
    assert!(!raw.contains("command"), "{raw}");
    assert!(!raw.contains("args"), "{raw}");
    assert!(!raw.contains("fake-agent.exe"), "a path leaked: {raw}");
    assert!(
        !raw.contains(&host.data_dir.to_string_lossy().to_string()),
        "a path leaked: {raw}"
    );

    // Unauthenticated: the uniform 401.
    let (status, body) = harness::raw_exchange(
        host.addr,
        b"GET /v1/catalog HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert_eq!(status, 401, "{body}");

    // A device without read_state fails closed (403).
    let (readerless, paired) = host.pair("no read state").await;
    let path = hn_core_host::paths::devices_path(&host.data_dir);
    let mut file = read_json(&path);
    for device in file["devices"].as_array_mut().expect("devices") {
        if device["device_id"] == paired.device_id.as_str() {
            device["permissions"] = json!([]);
        }
    }
    std::fs::write(&path, serde_json::to_string_pretty(&file).expect("json")).expect("write");
    let forbidden = readerless.catalog().await.expect_err("no read_state");
    assert_eq!(forbidden.status, 403, "{}", forbidden.body);
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_closes_the_pane_and_its_process() {
    let host = TestHost::start("stop").await;
    let client = host.paired_with("stopper", &[Permission::Respond]).await;
    let reader = host.paired_with("reader", &[]).await;

    let created = client
        .create_workspace("fake", &workdir(), "se va a parar")
        .await
        .expect("create");
    let pane_id = created["pane_id"].as_str().expect("pane_id").to_string();
    assert!(
        wait_for_status(&client, &pane_id, "blocked", Duration::from_secs(20)).await,
        "the agent never blocked"
    );

    // Without RESPOND the stop is refused before touching anything.
    let forbidden = reader.stop(&pane_id).await.expect_err("no respond");
    assert_eq!(forbidden.status, 403, "{}", forbidden.body);

    let pid = host.host.pid_of(&pane_id).expect("a live pid");
    assert!(process_exists(pid), "the agent should be running");

    // Watch the stream while the stop happens.
    let stream_client = client.clone();
    let stream = tokio::spawn(async move { stream_client.events(5).await });

    let stopped = client.stop(&pane_id).await.expect("stop");
    assert_eq!(stopped["ok"], true);
    assert_eq!(stopped["pane_id"], pane_id.as_str());
    assert!(
        wait_until_gone(&client, &pane_id, Duration::from_secs(10)).await,
        "the stopped pane is still listed"
    );

    // The child is gone, not orphaned.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while process_exists(pid) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!process_exists(pid), "the child process survived the stop");

    // Everything after the stop is `pane_gone`, including a second stop.
    let respond = client
        .request(
            "POST",
            &format!("/v1/agents/{pane_id}/respond"),
            Some(br#"{"text":"hola"}"#.to_vec()),
        )
        .await
        .expect("request");
    assert_eq!(respond.0, 400, "{}", respond.1);
    assert!(
        respond.1.contains("\"code\":\"pane_gone\""),
        "{}",
        respond.1
    );
    let again = client.stop(&pane_id).await.expect_err("already gone");
    assert_eq!(again.status, 400, "{}", again.body);
    assert!(
        again.body.contains("\"code\":\"pane_gone\""),
        "{}",
        again.body
    );
    let never = client.stop("w1:p99").await.expect_err("never existed");
    assert_eq!(never.status, 400, "{}", never.body);

    let captured = stream.await.expect("stream task").expect("stream");
    assert!(
        captured.contains("event: pane.closed"),
        "the stream missed the close: {captured}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn stopping_a_restored_pane_is_refused() {
    let host = TestHost::start("stop-restored").await;
    let client = host.paired_with("restorer", &[Permission::Respond]).await;
    let created = client
        .create_workspace("fake", &workdir(), "restaurado")
        .await
        .expect("create");
    let pane_id = created["pane_id"].as_str().expect("pane_id").to_string();
    assert!(
        wait_for_status(&client, &pane_id, "blocked", Duration::from_secs(20)).await,
        "the agent never blocked"
    );

    host.host.shutdown().await;
    let host = host.restart(Duration::from_secs(45)).await;
    let client = host
        .paired_with("restorer two", &[Permission::Respond])
        .await;

    let refused = client.stop(&pane_id).await.expect_err("restored");
    assert_eq!(refused.status, 409, "{}", refused.body);
    assert!(
        refused.body.contains("no running agent"),
        "{}",
        refused.body
    );
}

/// The host rewrites `agents.toml` between requests: adding, removing and
/// breaking entries must be visible without a restart, and a broken file must
/// keep the last good catalogue instead of taking the host down.
#[tokio::test(flavor = "multi_thread")]
async fn the_catalogue_reloads_without_a_restart() {
    let host = TestHost::start("catalog-reload").await;
    let client = host.paired_with("reloader", &[Permission::Respond]).await;
    let path = hn_core_host::paths::agents_path(&host.data_dir);
    let fake = env!("CARGO_BIN_EXE_fake-agent");

    // The harness catalogue starts with fake + missing.
    let catalog = client.catalog().await.expect("catalog");
    assert!(names(&catalog).contains(&"fake".to_string()));

    // Add an entry while the host runs: it appears and can be started.
    write_catalog_file(
        &path,
        &format!(
            "[agents.fake]\ncommand = '{fake}'\nargs = [\"--delay-ms\", \"150\"]\n\
             \n[agents.late]\ncommand = '{fake}'\nargs = [\"--delay-ms\", \"150\"]\n"
        ),
    )
    .await;
    let catalog = client.catalog().await.expect("catalog");
    assert!(names(&catalog).contains(&"late".to_string()));
    let created = client
        .create_workspace("late", &workdir(), "recien anadido")
        .await
        .expect("create the new entry");
    let pane_id = created["pane_id"].as_str().expect("pane_id").to_string();
    assert!(
        wait_for_status(&client, &pane_id, "blocked", Duration::from_secs(20)).await,
        "the new entry did not start"
    );
    client.stop(&pane_id).await.expect("stop");

    // Remove fake: it disappears from the catalogue and cannot be created.
    write_catalog_file(
        &path,
        &format!("[agents.late]\ncommand = '{fake}'\nargs = [\"--delay-ms\", \"150\"]\n"),
    )
    .await;
    let catalog = client.catalog().await.expect("catalog");
    assert!(!names(&catalog).contains(&"fake".to_string()));
    let unknown = client
        .create_workspace("fake", &workdir(), "borrado")
        .await
        .expect_err("removed entry");
    assert_eq!(unknown.status, 422, "{}", unknown.body);
    assert!(
        unknown.body.contains("\"code\":\"unknown_agent\""),
        "{}",
        unknown.body
    );

    // Break the file: the last good catalogue stays, the warning says why,
    // and creation still works with it.
    write_catalog_file(&path, "this is not toml").await;
    let catalog = client.catalog().await.expect("catalog");
    assert!(names(&catalog).contains(&"late".to_string()), "{catalog}");
    assert!(catalog["warning"].is_string(), "{catalog}");
    let still = client
        .create_workspace("late", &workdir(), "sigue")
        .await
        .expect("the last good catalogue still works");
    let pane_id = still["pane_id"].as_str().expect("pane_id").to_string();
    client.stop(&pane_id).await.expect("stop");

    // Fix it: the warning goes away and the new entries are live.
    write_catalog_file(&path, &format!("[agents.fixed]\ncommand = '{fake}'\n")).await;
    let catalog = client.catalog().await.expect("catalog");
    assert!(names(&catalog).contains(&"fixed".to_string()), "{catalog}");
    assert!(catalog.get("warning").is_none(), "{catalog}");
}

fn names(catalog: &Value) -> Vec<String> {
    catalog["agents"]
        .as_array()
        .map(|agents| {
            agents
                .iter()
                .filter_map(|agent| agent["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Write the file and make sure its mtime is visibly newer than the last read.
async fn write_catalog_file(path: &std::path::Path, text: &str) {
    tokio::time::sleep(Duration::from_millis(30)).await;
    std::fs::write(path, text).expect("write agents.toml");
    tokio::time::sleep(Duration::from_millis(30)).await;
}
