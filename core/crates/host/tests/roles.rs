//! `GET /v1/roles` through the real host: the role catalogue, read-only and
//! the same on every read, behind the signed read grant.

mod harness;

use std::collections::HashSet;

use harness::TestHost;
use serde_json::{json, Value};

fn read_json(path: &std::path::Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("json")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_role_catalogue_is_served_to_readers_and_is_stable() {
    let host = TestHost::start("roles").await;
    let client = host.paired_with("role reader", &[]).await;

    let (status, first) = client
        .request("GET", "/v1/roles", None)
        .await
        .expect("roles");
    assert_eq!(status, 200, "{first}");
    let (status, second) = client
        .request("GET", "/v1/roles", None)
        .await
        .expect("roles");
    assert_eq!(status, 200, "{second}");
    assert_eq!(first, second, "two reads of the same catalogue differ");

    let body: Value = serde_json::from_str(&first).expect("json");
    let keys: HashSet<&str> = body
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        HashSet::from(["ok", "version", "areas", "roles"]),
        "{body}"
    );
    assert_eq!(body["ok"], true);
    assert_eq!(body["version"], 1);

    let areas = body["areas"].as_array().expect("areas");
    let roles = body["roles"].as_array().expect("roles");
    assert_eq!(areas.len(), 9);
    assert_eq!(roles.len(), 43);
    let role_ids: HashSet<&str> = roles
        .iter()
        .filter_map(|role| role["id"].as_str())
        .collect();
    assert_eq!(role_ids.len(), roles.len(), "role ids repeat");
    assert!(!role_ids.contains("alfred"), "Alfred is fixed, never hired");
    for role in roles {
        let area = role["area"].as_str().expect("area");
        assert!(
            areas.iter().any(|candidate| candidate["id"] == area),
            "{role}"
        );
        assert!(
            role["title"].as_str().is_some_and(|t| !t.is_empty()),
            "{role}"
        );
    }
    for area in areas {
        let equipo = area["levels"]["equipo"].as_array().expect("equipo");
        assert!(equipo.len() <= 8, "{area}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_role_catalogue_refuses_the_unauthenticated_and_the_readerless() {
    let host = TestHost::start("roles-refused").await;

    // Unauthenticated: the uniform 401.
    let (status, body) = harness::raw_exchange(
        host.addr,
        b"GET /v1/roles HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
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
    let (status, body) = readerless
        .request("GET", "/v1/roles", None)
        .await
        .expect("roles");
    assert_eq!(status, 403, "{body}");
}
