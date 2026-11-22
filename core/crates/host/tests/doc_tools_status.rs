//! `GET /v1/doc-tools` (DOC7A): the document CLIs this PC has.

mod harness;

use excubia_crafting::App;
use harness::TestHost;
use serde_json::Value;

fn read_json(path: &std::path::Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("json")
}

fn tool<'a>(body: &'a Value, app: &str) -> &'a Value {
    body["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["app"] == app)
        .unwrap_or_else(|| panic!("no {app} in {body}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn the_host_lists_the_three_document_tools_without_paths() {
    let host = TestHost::start_with_docs("doc-tools", &[App::GridCraft, App::PdfCraft]).await;
    let client = host.paired_with("doc tools reader", &[]).await;

    let (status, body) = client
        .request("GET", "/v1/doc-tools", None)
        .await
        .expect("doc tools");
    assert_eq!(status, 200, "{body}");
    let body: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(body["ok"], true);

    // Always the three, in App::ALL order.
    let apps: Vec<&str> = body["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["app"].as_str().expect("app"))
        .collect();
    assert_eq!(apps, ["gridcraft", "wordcraft", "pdfcraft"]);

    let gridcraft = tool(&body, "gridcraft");
    assert_eq!(gridcraft["name"], "GridCraft");
    assert_eq!(gridcraft["available"], true);
    assert_eq!(gridcraft["tested_version"], "0.16.1");

    // Missing, but its tested version is still known and shown.
    let wordcraft = tool(&body, "wordcraft");
    assert_eq!(wordcraft["name"], "WordCraft");
    assert_eq!(wordcraft["available"], false);
    assert_eq!(wordcraft["tested_version"], "0.15.2");

    let pdfcraft = tool(&body, "pdfcraft");
    assert_eq!(pdfcraft["name"], "PdfCraft");
    assert_eq!(pdfcraft["available"], true);
    assert_eq!(pdfcraft["tested_version"], "0.18.2");

    // Names and availability only: never a command or a path.
    let raw = body.to_string();
    assert!(!raw.contains("command"), "{raw}");
    assert!(!raw.contains("args"), "{raw}");
    assert!(!raw.contains("fake-bin"), "a path leaked: {raw}");
    assert!(!raw.contains(".exe"), "a path leaked: {raw}");
    assert!(
        !raw.contains(&host.data_dir.to_string_lossy().to_string()),
        "a path leaked: {raw}"
    );

    // A device without read_state gets the same refusal as /v1/catalog.
    let (readerless, paired) = host.pair("no read state").await;
    let path = excubia_host::paths::devices_path(&host.data_dir);
    let mut file = read_json(&path);
    for device in file["devices"].as_array_mut().expect("devices") {
        if device["device_id"] == paired.device_id.as_str() {
            device["permissions"] = serde_json::json!([]);
        }
    }
    std::fs::write(&path, serde_json::to_string_pretty(&file).expect("json")).expect("write");

    let (status, refused) = readerless
        .request("GET", "/v1/doc-tools", None)
        .await
        .expect("request");
    assert_eq!(status, 403, "{refused}");
    let catalog = readerless.catalog().await.expect_err("no read_state");
    assert_eq!(catalog.status, 403, "{}", catalog.body);
    assert_eq!(refused, catalog.body);
}
