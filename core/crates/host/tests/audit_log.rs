//! The audit log over the real routes (`docs/contracts/audit.md`): each connected action leaves its
//! line, and a log that cannot be written never fails the request.

mod harness;

use excubia_auth::Permission;
use excubia_host::audit::AUDIT_FILE;
use excubia_test_support::client::SignedClient;
use excubia_test_support::{pair_device, DemoIdentity};
use harness::TestHost;
use serde_json::{json, Value};

const ALFRED: &str = "/v1/alfred/backend";

/// A paired device with the given grants, and its id.
async fn paired(host: &TestHost, name: &str, grants: &[Permission]) -> (SignedClient, String) {
    let (client, paired) = host.pair(name).await;
    host.grant(&paired.device_id, Permission::ReadState);
    for permission in grants {
        host.grant(&paired.device_id, *permission);
    }
    (client, paired.device_id)
}

fn lines(host: &TestHost) -> Vec<Value> {
    let text = std::fs::read_to_string(host.data_dir.join(AUDIT_FILE)).unwrap_or_default();
    text.lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

fn of_action<'a>(all: &'a [Value], action: &str) -> Vec<&'a Value> {
    all.iter().filter(|line| line["action"] == action).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_engine_change_leaves_one_line_with_the_device_and_the_engine() {
    let host = TestHost::start("audit-alfred").await;
    let (agent, agent_id) = paired(&host, "Agente", &[Permission::Approve]).await;

    let (status, text) = agent
        .request("POST", ALFRED, Some(br#"{"backend":"local"}"#.to_vec()))
        .await
        .expect("post");
    assert_eq!(status, 202, "{text}");

    let all = lines(&host);
    let changed = of_action(&all, "alfred.backend");
    assert_eq!(changed.len(), 1, "{all:?}");
    assert_eq!(changed[0]["device"], agent_id.as_str());
    assert_eq!(changed[0]["outcome"], "started");
    assert_eq!(changed[0]["detail"]["backend"], "local");
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_grant_and_revoke_each_leave_their_line() {
    let host = TestHost::start("audit-pairing").await;
    let (admin, admin_id) = paired(&host, "Admin", &[Permission::Admin]).await;

    let code = host.pair_code();
    let phone = DemoIdentity::generate("", "Phone");
    let paired_phone = pair_device(
        &host.base_url(),
        "p256",
        &phone.public_key_b64(),
        "Phone",
        &code,
    )
    .await
    .expect("pair");
    let stranger = DemoIdentity::generate("", "Intruso");
    let refused = pair_device(
        &host.base_url(),
        "p256",
        &stranger.public_key_b64(),
        "Intruso",
        "ZZZZZZZ",
    )
    .await;
    assert!(refused.is_err(), "a wrong code is refused");

    let grant = format!("/v1/devices/{}/grant", paired_phone.device_id);
    let (status, text) = admin
        .request(
            "POST",
            &grant,
            Some(serde_json::to_vec(&json!({"permissions": ["respond"]})).expect("json")),
        )
        .await
        .expect("grant");
    assert_eq!(status, 200, "{text}");
    admin
        .revoke(&paired_phone.device_id)
        .await
        .expect("revoke the phone");

    let all = lines(&host);
    let pairs = of_action(&all, "pair");
    let phone_line = pairs
        .iter()
        .find(|line| line["device"] == paired_phone.device_id.as_str())
        .expect("the phone's pairing line");
    assert_eq!(phone_line["outcome"], "paired");
    assert_eq!(phone_line["detail"]["via"], "code");
    assert!(
        pairs
            .iter()
            .any(|line| line["outcome"] == "rejected" && line["device"].is_null()),
        "{all:?}"
    );

    let granted = of_action(&all, "devices.grant");
    assert_eq!(granted.len(), 1, "{all:?}");
    assert_eq!(granted[0]["device"], paired_phone.device_id.as_str());
    assert_eq!(granted[0]["detail"]["by"], admin_id.as_str());
    assert_eq!(granted[0]["detail"]["permissions"], "respond");

    let revoked = of_action(&all, "devices.revoke");
    assert_eq!(revoked.len(), 1, "{all:?}");
    assert_eq!(revoked[0]["device"], paired_phone.device_id.as_str());
    assert_eq!(revoked[0]["detail"]["by"], admin_id.as_str());
    assert_eq!(revoked[0]["outcome"], "revoked");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_log_that_cannot_be_written_does_not_fail_the_request() {
    let host = TestHost::start("audit-blocked").await;
    // A directory where the log file goes: every append fails.
    std::fs::create_dir(host.data_dir.join(AUDIT_FILE)).expect("blocker");
    let (agent, _) = paired(&host, "Agente", &[Permission::Approve]).await;

    let (status, text) = agent
        .request("POST", ALFRED, Some(br#"{"backend":"local"}"#.to_vec()))
        .await
        .expect("post");
    assert_eq!(status, 202, "{text}");
}
