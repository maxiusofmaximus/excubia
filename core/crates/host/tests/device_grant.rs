//! `POST /v1/devices/{id}/grant` and the verification string (SAS) of a pending
//! device: the list shows it, the pairing notice carries it, and the grant leaves
//! one audit line and one notice.

mod harness;

use base64::Engine as _;
use harness::TestHost;
use excubia_core_auth::Permission;
use excubia_core_chat::{ChatStore, ConversationFilter};
use excubia_core_host::pairing_audit::AUDIT_FILE;
use excubia_core_host::pairing_notice::NOTICE_TITLE;
use excubia_core_pin::sas;
use serde_json::{json, Value};

fn grant_path(device_id: &str) -> String {
    format!("/v1/devices/{device_id}/grant")
}

fn body(permissions: &[&str]) -> Option<Vec<u8>> {
    Some(serde_json::to_vec(&json!({ "permissions": permissions })).expect("json"))
}

fn notice_texts(chat: &ChatStore) -> Vec<String> {
    let filter = ConversationFilter {
        q: None,
        section_id: None,
        pinned: false,
        archived: false,
        attention: false,
        working: false,
        cursor: None,
        limit: 100,
    };
    chat.list_conversations(filter)
        .expect("list conversations")
        .conversations
        .into_iter()
        .filter(|conversation| conversation.title == NOTICE_TITLE)
        .flat_map(|conversation| {
            chat.messages(&conversation.id, None, None, 100)
                .expect("messages")
                .messages
        })
        .map(|message| message.body_md)
        .collect()
}

fn audit_lines(host: &TestHost) -> Vec<Value> {
    std::fs::read_to_string(host.data_dir.join(AUDIT_FILE))
        .expect("audit file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

/// The SAS a phone shows for its key: the host pin and the SEC1 bytes it sent.
fn expected_sas(host: &TestHost, public_key_b64: &str) -> String {
    let key = base64::engine::general_purpose::STANDARD
        .decode(public_key_b64)
        .expect("base64 key");
    sas(&host.host_pin, &key)
}

#[tokio::test(flavor = "multi_thread")]
async fn an_admin_grants_a_pending_device_and_the_grant_is_audited_and_announced() {
    let host = TestHost::start("grant-ok").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let (_tablet, pending) = host.pair("Tablet").await;

    let (status, text) = admin
        .request(
            "POST",
            &grant_path(&pending.device_id),
            body(&["respond", "read_state"]),
        )
        .await
        .expect("grant");
    assert_eq!(status, 200, "{text}");
    let device: Value = serde_json::from_str(&text).expect("device");
    assert_eq!(device["device_id"], pending.device_id.as_str());
    assert_eq!(device["permissions"], json!(["read_state", "respond"]));

    let granted = audit_lines(&host).pop().expect("a line");
    assert_eq!(granted["outcome"], "granted");
    assert_eq!(granted["via"], "admin");
    assert_eq!(granted["device_id"], pending.device_id.as_str());
    assert_eq!(granted["granted_by"], admin.identity().device_id());
    assert_eq!(granted["permissions"], json!(["read_state", "respond"]));

    let texts = notice_texts(&host.chat);
    let last = texts.last().expect("a notice");
    assert_eq!(
        last,
        &format!(
            "`{}` concedió read_state, respond a `Tablet` ({}).",
            admin.identity().device_id(),
            pending.device_id
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_without_admin_cannot_grant() {
    let host = TestHost::start("grant-403").await;
    let reader = host.paired_with("Reader", &[]).await;
    let (_tablet, pending) = host.pair("Tablet").await;

    let (status, _) = reader
        .request("POST", &grant_path(&pending.device_id), body(&["respond"]))
        .await
        .expect("grant");
    assert_eq!(status, 403);
    assert!(audit_lines(&host)
        .iter()
        .all(|line| line["outcome"] != "granted"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_cannot_grant_itself() {
    let host = TestHost::start("grant-self").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let own = admin.identity().device_id().to_owned();

    let (status, text) = admin
        .request("POST", &grant_path(&own), body(&["respond"]))
        .await
        .expect("grant");
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("cannot_grant_self"), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_that_already_has_permissions_answers_409() {
    let host = TestHost::start("grant-409").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let phone = host.paired_with("Phone", &[Permission::Respond]).await;

    let (status, text) = admin
        .request(
            "POST",
            &grant_path(phone.identity().device_id()),
            body(&["send_key"]),
        )
        .await
        .expect("grant");
    assert_eq!(status, 409, "{text}");
    assert!(text.contains("not_pending"), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_is_never_granted_over_http() {
    let host = TestHost::start("grant-admin").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let (_tablet, pending) = host.pair("Tablet").await;

    let (status, text) = admin
        .request(
            "POST",
            &grant_path(&pending.device_id),
            body(&["read_state", "admin"]),
        )
        .await
        .expect("grant");
    assert_eq!(status, 400, "{text}");
    assert!(text.contains("admin_not_grantable"), "{text}");

    let still = host.devices.get(&pending.device_id).expect("device");
    assert!(
        still.permissions.is_empty(),
        "a refused grant grants nothing"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_or_revoked_device_answers_404() {
    let host = TestHost::start("grant-404").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let (_tablet, pending) = host.pair("Tablet").await;
    admin
        .revoke(&pending.device_id)
        .await
        .expect("revoke the pending device");

    for id in ["dev-missing", pending.device_id.as_str()] {
        let (status, text) = admin
            .request("POST", &grant_path(id), body(&["respond"]))
            .await
            .expect("grant");
        assert_eq!(status, 404, "{id}: {text}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_malformed_permission_list_answers_422() {
    let host = TestHost::start("grant-422").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let (_tablet, pending) = host.pair("Tablet").await;
    let path = grant_path(&pending.device_id);

    for payload in [body(&["shout"]), body(&[])] {
        let (status, text) = admin.request("POST", &path, payload).await.expect("grant");
        assert_eq!(status, 422, "{text}");
        assert!(text.contains("permissions"), "{text}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pending_device_shows_its_sas_and_the_pairing_notice_carries_the_same() {
    let host = TestHost::start("sas-list").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let (tablet, pending) = host.pair("Tablet").await;
    let want = expected_sas(&host, &tablet.identity().public_key_b64());

    let listed = admin.devices().await.expect("devices");
    let devices = listed["devices"].as_array().expect("array");
    let entry = devices
        .iter()
        .find(|device| device["device_id"] == pending.device_id.as_str())
        .expect("the pending device is listed");
    assert_eq!(entry["sas"], want.as_str());
    assert!(
        devices
            .iter()
            .filter(|device| device["device_id"] != pending.device_id.as_str())
            .all(|device| device.get("sas").is_none()),
        "only pending devices carry a sas"
    );

    let texts = notice_texts(&host.chat);
    let notice = texts
        .iter()
        .find(|text| {
            text.starts_with("Nuevo dispositivo pendiente") && text.contains(&pending.device_id)
        })
        .expect("the pairing notice");
    assert!(
        notice.contains(&format!("Cadena de verificación: **{want}**")),
        "{notice}"
    );

    let paired_line = audit_lines(&host)
        .into_iter()
        .find(|line| line["outcome"] == "paired")
        .expect("a paired line");
    assert!(
        paired_line.get("sas").is_none(),
        "the audit never carries the sas"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_granted_device_no_longer_shows_a_sas() {
    let host = TestHost::start("sas-granted").await;
    let admin = host.paired_with("Admin", &[Permission::Admin]).await;
    let (_tablet, pending) = host.pair("Tablet").await;
    let (status, _) = admin
        .request(
            "POST",
            &grant_path(&pending.device_id),
            body(&["read_state"]),
        )
        .await
        .expect("grant");
    assert_eq!(status, 200);

    let listed = admin.devices().await.expect("devices");
    let entry = listed["devices"]
        .as_array()
        .expect("array")
        .iter()
        .find(|device| device["device_id"] == pending.device_id.as_str())
        .expect("listed");
    assert!(entry.get("sas").is_none());
}
