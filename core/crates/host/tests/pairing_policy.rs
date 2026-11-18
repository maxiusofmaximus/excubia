//! The tailnet pairing policy (`tailnet_auto_accept`). By default a code-less
//! `POST /v1/pair` from the host's own tailnet owner leaves the device pending;
//! with the policy on, it gets READ_STATE, RESPOND and SEND_KEY and nothing more.
//! A stranger and a wrong code are refused either way.

mod harness;

use std::sync::Arc;

use excubia_api::TailnetOwners;
use excubia_auth::Permission;
use excubia_host::audit::AUDIT_FILE as AUDIT;
use excubia_host::pairing_audit::AUDIT_FILE as PAIRING_AUDIT;
use excubia_test_support::{pair_device, DemoIdentity, HttpError, PairResponse};
use harness::TestHost;
use serde_json::{json, Value};

const POLICY: &str = "/v1/pairing/policy";

/// Who owns the peer, and who owns the host, as the Tailscale CLI would say.
struct Tailnet {
    peer: &'static str,
}

impl TailnetOwners for Tailnet {
    fn owner_of(&self, _ip: &str) -> Option<String> {
        Some(self.peer.to_string())
    }

    fn host_owner(&self) -> Option<String> {
        Some("owner-1".to_string())
    }
}

/// A host whose peer belongs to `peer`: `owner-1` is the host's own account.
async fn host_with_peer(name: &str, peer: &'static str) -> TestHost {
    TestHost::start_with_tailnet(name, Arc::new(Tailnet { peer })).await
}

/// A `POST /v1/pair` with no code: the tailnet path.
async fn pair_from_tailnet(host: &TestHost, name: &str) -> Result<PairResponse, HttpError> {
    let identity = DemoIdentity::generate("", name);
    pair_device(
        &host.base_url(),
        "p256",
        &identity.public_key_b64(),
        name,
        "",
    )
    .await
}

fn lines_of(host: &TestHost, file: &str) -> Vec<Value> {
    std::fs::read_to_string(host.data_dir.join(file))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

fn json_body(value: Value) -> Option<Vec<u8>> {
    Some(serde_json::to_vec(&value).expect("json"))
}

#[tokio::test(flavor = "multi_thread")]
async fn by_default_a_tailnet_pairing_leaves_the_device_pending() {
    let host = host_with_peer("policy-default", "owner-1").await;

    let paired = pair_from_tailnet(&host, "Phone").await.expect("pair");
    assert!(paired.permissions.is_empty(), "{:?}", paired.permissions);
    let stored = host.devices.get(&paired.device_id).expect("the device");
    assert!(stored.permissions.is_empty());

    let lines = lines_of(&host, PAIRING_AUDIT);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["outcome"], "paired");
    assert_eq!(lines[0]["via"], "tailnet");
}

#[tokio::test(flavor = "multi_thread")]
async fn with_the_policy_on_a_tailnet_pairing_gets_the_reader_set_and_nothing_more() {
    let host = host_with_peer("policy-auto", "owner-1").await;
    host.devices.set_tailnet_auto_accept(true).expect("policy");

    let paired = pair_from_tailnet(&host, "Phone").await.expect("pair");
    assert_eq!(
        paired.permissions,
        vec!["read_state", "respond", "send_key"]
    );
    let stored = host.devices.get(&paired.device_id).expect("the device");
    assert_eq!(
        stored.permissions,
        vec![
            Permission::ReadState,
            Permission::Respond,
            Permission::SendKey
        ]
    );

    let lines = lines_of(&host, PAIRING_AUDIT);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0]["outcome"], "paired");
    assert_eq!(lines[1]["outcome"], "granted");
    assert_eq!(lines[1]["via"], "tailnet");
    assert_eq!(lines[1]["granted_by"], "auto-tailnet");
    assert_eq!(lines[1]["device_id"], paired.device_id.as_str());
    assert_eq!(
        lines[1]["permissions"],
        json!(["read_state", "respond", "send_key"])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn with_the_policy_on_a_stranger_and_a_wrong_code_are_still_refused() {
    let host = host_with_peer("policy-stranger", "stranger").await;
    host.devices.set_tailnet_auto_accept(true).expect("policy");

    let refused = pair_from_tailnet(&host, "Intruso")
        .await
        .expect_err("another account is refused");
    assert_eq!(refused.status, 403, "{}", refused.body);
    assert!(
        refused.body.contains("pairing_rejected"),
        "{}",
        refused.body
    );

    let wrong = pair_device(
        &host.base_url(),
        "p256",
        &DemoIdentity::generate("", "Intruso").public_key_b64(),
        "Intruso",
        "ZZZZZZZ",
    )
    .await
    .expect_err("a wrong code is refused");
    assert_eq!(wrong.status, 403, "{}", wrong.body);
    assert!(wrong.body.contains("pairing_rejected"), "{}", wrong.body);

    assert!(host.devices.list().is_empty(), "no device was made");
    assert!(lines_of(&host, PAIRING_AUDIT)
        .iter()
        .all(|line| line["outcome"] == "rejected"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_policy_is_admin_only_strict_and_audited() {
    let host = host_with_peer("policy-http", "owner-1").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let reader = host.paired_with("Reader", &[Permission::Respond]).await;

    let (status, _) = reader
        .request(
            "PUT",
            POLICY,
            json_body(json!({"tailnet_auto_accept": true})),
        )
        .await
        .expect("put");
    assert_eq!(status, 403);
    assert!(
        !host.devices.tailnet_auto_accept(),
        "a refused change changes nothing"
    );

    let (status, text) = admin
        .request(
            "PUT",
            POLICY,
            json_body(json!({"tailnet_auto_accept": true, "extra": 1})),
        )
        .await
        .expect("put");
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("invalid_body"), "{text}");
    assert!(!host.devices.tailnet_auto_accept());

    let (status, text) = admin
        .request(
            "PUT",
            POLICY,
            json_body(json!({"tailnet_auto_accept": true})),
        )
        .await
        .expect("put");
    assert_eq!(status, 200, "{text}");
    assert!(host.devices.tailnet_auto_accept());

    let (status, text) = admin.request("GET", POLICY, None).await.expect("get");
    assert_eq!(status, 200, "{text}");
    let read: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(read["tailnet_auto_accept"], true);

    let change = lines_of(&host, AUDIT).pop().expect("the change is audited");
    assert_eq!(change["action"], "pairing_policy");
    assert_eq!(change["device"], admin.identity().device_id());
    assert_eq!(change["detail"]["tailnet_auto_accept"], "true");
}
