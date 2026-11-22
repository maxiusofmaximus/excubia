//! Restore points over HTTP (F2-CORE). A turn of an agent keeps its folder as it
//! was; the listing names the files without their content; putting a point back
//! needs `APPROVE`, and the undo of that restore is a point too. A device without
//! `APPROVE`, an unknown id and a path with `..` are refused, and a refused
//! restore writes nothing.

mod harness;

use std::fs;
use std::path::Path;

use excubia_auth::Permission;
use excubia_host::SpawnSpec;
use excubia_test_dir::TestDir;
use harness::TestHost;
use serde_json::Value;

/// A point id that is well formed and was never issued.
const UNKNOWN_POINT: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAZ";

fn answered(status: u16, body: &str) -> Value {
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(body).expect("json body")
}

/// Spawn the fake agent with `folder` as its working directory: its turns keep that folder.
async fn spawn_in(host: &TestHost, folder: &Path) -> String {
    let spec = SpawnSpec {
        program: env!("CARGO_BIN_EXE_fake-agent").to_string(),
        args: Vec::new(),
        cwd: Some(folder.to_path_buf()),
        label: None,
        env: Vec::new(),
    };
    host.host.start(spec).await.expect("spawn")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_is_kept_listed_restored_and_undone_over_http() {
    let host = TestHost::start("restore-http").await;
    let work = TestDir::new("restore-http-work");
    let folder = work.path();
    fs::write(folder.join("a.txt"), "uno").expect("a");
    fs::write(folder.join("keep.txt"), "quedo").expect("keep");
    let (client, paired) = host.pair("Escritorio restore").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    host.grant(&paired.device_id, Permission::Approve);
    let pane = spawn_in(&host, folder).await;

    // The turn starts: the folder as it is now becomes the point. Then the agent edits it.
    client.respond(&pane, "hola").await.expect("respond");
    fs::write(folder.join("a.txt"), "dos!!").expect("edit");
    fs::write(folder.join("b.txt"), "extra").expect("new");

    let (status, body) = client
        .request("GET", &format!("/v1/restore/points?agent={pane}"), None)
        .await
        .expect("list");
    let listed = answered(status, &body);
    let points = listed["points"].as_array().expect("points");
    assert_eq!(points.len(), 1, "{body}");
    assert_eq!(points[0]["files"], 2, "{body}");
    let point = points[0]["id"].as_str().expect("id").to_string();

    let (status, body) = client
        .request("GET", &format!("/v1/restore/points/{point}/files"), None)
        .await
        .expect("files");
    let changes = answered(status, &body);
    let listing = changes["changes"].as_array().expect("changes");
    let status_of = |path: &str| {
        listing
            .iter()
            .find(|c| c["path"] == path)
            .map(|c| c["status"].as_str().unwrap_or_default().to_string())
    };
    assert_eq!(status_of("a.txt").as_deref(), Some("modified"), "{body}");
    assert_eq!(status_of("b.txt").as_deref(), Some("added"), "{body}");
    assert!(
        !body.contains("dos!!") && !body.contains("extra"),
        "no content: {body}"
    );

    let (status, body) = client
        .request("POST", &format!("/v1/restore/points/{point}/restore"), None)
        .await
        .expect("restore");
    let done = answered(status, &body);
    assert_eq!(done["restored"], 1, "{body}");
    assert_eq!(done["removed"], 1, "{body}");
    let undo = done["undo_point"].as_str().expect("undo point").to_string();
    assert_eq!(fs::read_to_string(folder.join("a.txt")).expect("a"), "uno");
    assert!(!folder.join("b.txt").exists());

    // The undo of the restore is a point of its own, and putting it back returns the edits.
    let (status, body) = client
        .request("POST", &format!("/v1/restore/points/{undo}/restore"), None)
        .await
        .expect("undo");
    let undone = answered(status, &body);
    assert_eq!(undone["restored"], 2, "{body}");
    assert_eq!(
        fs::read_to_string(folder.join("a.txt")).expect("a"),
        "dos!!"
    );
    assert_eq!(
        fs::read_to_string(folder.join("b.txt")).expect("b"),
        "extra"
    );

    let (status, body) = client
        .request("GET", &format!("/v1/restore/points?agent={pane}"), None)
        .await
        .expect("list again");
    assert_eq!(
        answered(status, &body)["points"].as_array().map(Vec::len),
        Some(3)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restore_is_refused_without_approve_and_for_bad_ids() {
    let host = TestHost::start("restore-refused").await;
    let work = TestDir::new("restore-refused-work");
    let folder = work.path();
    fs::write(folder.join("a.txt"), "uno").expect("a");
    let (acting, acting_paired) = host.pair("Con respond").await;
    host.grant(&acting_paired.device_id, Permission::ReadState);
    host.grant(&acting_paired.device_id, Permission::Respond);
    let (reader, reader_paired) = host.pair("Solo lectura").await;
    host.grant(&reader_paired.device_id, Permission::ReadState);
    let pane = spawn_in(&host, folder).await;
    acting.respond(&pane, "hola").await.expect("respond");

    let (status, body) = acting
        .request("GET", &format!("/v1/restore/points?agent={pane}"), None)
        .await
        .expect("list");
    let point = answered(status, &body)["points"][0]["id"]
        .as_str()
        .expect("point")
        .to_string();
    fs::write(folder.join("a.txt"), "cambio").expect("edit");

    // Reading is READ_STATE; putting a point back is APPROVE, which neither device holds.
    let (status, body) = reader
        .request("GET", &format!("/v1/restore/points/{point}/files"), None)
        .await
        .expect("reader files");
    answered(status, &body);
    for client in [&reader, &acting] {
        let (status, body) = client
            .request("POST", &format!("/v1/restore/points/{point}/restore"), None)
            .await
            .expect("refused restore");
        assert_eq!(status, 403, "{body}");
        assert!(body.contains("forbidden"), "{body}");
    }
    assert_eq!(
        fs::read_to_string(folder.join("a.txt")).expect("a"),
        "cambio"
    );

    host.grant(&acting_paired.device_id, Permission::Approve);
    let (status, body) = acting
        .request(
            "POST",
            &format!("/v1/restore/points/{UNKNOWN_POINT}/restore"),
            None,
        )
        .await
        .expect("unknown");
    assert_eq!(status, 404, "{body}");
    assert!(body.contains("restore_point_not_found"), "{body}");

    // A point id that climbs out of the store is not a point id at all.
    let (status, body) = acting
        .request("GET", "/v1/restore/points/..%2F..%2Fetc/files", None)
        .await
        .expect("traversal files");
    assert_eq!(status, 404, "{body}");
    let (status, body) = acting
        .request("POST", "/v1/restore/points/..%2F..%2Fetc/restore", None)
        .await
        .expect("traversal restore");
    assert_eq!(status, 404, "{body}");
    assert_eq!(
        fs::read_to_string(folder.join("a.txt")).expect("a"),
        "cambio"
    );
}
