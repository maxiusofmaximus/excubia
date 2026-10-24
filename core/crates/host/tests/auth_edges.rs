//! Pairing and authentication failures, plus the contract's typed action
//! errors. Every assertion here is a row of `errors.md` / `auth.md`.

mod harness;

use std::time::Duration;

use harness::TestHost;
use hn_core_auth::{canonical_string, Permission};
use hn_core_test_support::{pair_device, DemoIdentity, HttpError};

#[tokio::test(flavor = "multi_thread")]
async fn pairing_failures_follow_the_contract() {
    let host = TestHost::start("pairing").await;
    let identity = DemoIdentity::generate("", "pairing test");
    let url = host.base_url();

    // No code from loopback: refused with the {code, message} shape.
    let refused = pair_device(&url, "p256", &identity.public_key_b64(), "PC", "")
        .await
        .expect_err("no code");
    assert_eq!(refused.status, 403, "{}", refused.body);
    assert!(
        refused.body.contains("\"code\":\"pairing_rejected\""),
        "{}",
        refused.body
    );
    assert!(refused.body.contains("host pair-code"), "{}", refused.body);

    // An unsupported curve and an oversized label are model failures.
    let bad_curve = pair_device(
        &url,
        "rsa",
        &identity.public_key_b64(),
        "PC",
        &host.pair_code(),
    )
    .await
    .expect_err("curve");
    assert_eq!(bad_curve.status, 400, "{}", bad_curve.body);
    let long_label = "a".repeat(65);
    let bad_label = pair_device(
        &url,
        "p256",
        &identity.public_key_b64(),
        &long_label,
        &host.pair_code(),
    )
    .await
    .expect_err("label");
    assert_eq!(bad_label.status, 400, "{}", bad_label.body);

    // A valid code pairs once; the same code again is the uniform refusal.
    let code = host.pair_code();
    let paired = pair_device(&url, "p256", &identity.public_key_b64(), "PC", &code)
        .await
        .expect("pair");
    // A new device is pending: it holds nothing until the host grants it (D3).
    assert!(paired.permissions.is_empty(), "{:?}", paired.permissions);
    let reused = pair_device(&url, "p256", &identity.public_key_b64(), "PC", &code)
        .await
        .expect_err("reused");
    assert_eq!(reused.status, 403, "{}", reused.body);
    assert!(reused.body.contains("pairing_rejected"), "{}", reused.body);

    // An expired code is refused too.
    let expired = host
        .devices
        .issue_enrollment_code(0.0)
        .expect("issue expired");
    tokio::time::sleep(Duration::from_millis(20)).await;
    let expired_response = pair_device(&url, "p256", &identity.public_key_b64(), "PC", &expired)
        .await
        .expect_err("expired");
    assert_eq!(expired_response.status, 403, "{}", expired_response.body);

    // A recovery code re-keys the device and rotates the code.
    let recovery = paired.recovery_code.clone().expect("recovery");
    let replacement = DemoIdentity::generate("", "replacement");
    let rekeyed = pair_device(&url, "p256", &replacement.public_key_b64(), "PC", &recovery)
        .await
        .expect("rekey");
    assert_eq!(rekeyed.device_id, paired.device_id);
    assert_ne!(rekeyed.recovery_code, paired.recovery_code);
}

#[tokio::test(flavor = "multi_thread")]
async fn signature_freshness_and_replay_failures_are_uniform_401() {
    let host = TestHost::start("auth-fail").await;
    let (client, paired) = host.pair("auth failures").await;
    host.grant(&paired.device_id, Permission::ReadState);
    let identity = client.identity().clone();

    // No headers at all.
    let (status, body) = raw_request(&host, "GET", "/v1/state", None).await;
    assert_eq!(status, 401, "{body}");
    assert!(body.contains("unauthorized"), "{body}");

    // A device the trust store has never seen, with a perfectly good signature.
    let ghost = DemoIdentity::generate("dev-ghost", "ghost");
    let (status, _) = signed_raw(&host, &ghost, "GET", "/v1/state", b"", None, None).await;
    assert_eq!(status, 401, "unknown device");

    // A signature over different bytes.
    let timestamp = now();
    let nonce = "bm9uY2UtYmFk";
    let wrong = canonical_string("GET", "/v1/other", b"", timestamp, nonce).expect("canonical");
    let (status, _) = signed_raw(
        &host,
        &identity,
        "GET",
        "/v1/state",
        b"",
        Some(timestamp),
        Some((nonce.to_string(), wrong)),
    )
    .await;
    assert_eq!(status, 401, "bad signature");

    // A timestamp outside the 60 s window.
    let (status, _) = signed_raw(
        &host,
        &identity,
        "GET",
        "/v1/state",
        b"",
        Some(now() - 120),
        None,
    )
    .await;
    assert_eq!(status, 401, "stale timestamp");

    // A replayed nonce: the first use succeeds, the second is refused.
    let nonce = "bm9uY2UtcmVwbGF5";
    let (first, _) = signed_raw(
        &host,
        &identity,
        "GET",
        "/v1/state",
        b"",
        None,
        Some((nonce.to_string(), String::new())),
    )
    .await;
    assert_eq!(first, 200, "first use of a nonce");
    let (second, _) = signed_raw(
        &host,
        &identity,
        "GET",
        "/v1/state",
        b"",
        None,
        Some((nonce.to_string(), String::new())),
    )
    .await;
    assert_eq!(second, 401, "replayed nonce");
}

#[tokio::test(flavor = "multi_thread")]
async fn authorization_grants_and_typed_action_errors() {
    let host = TestHost::start("authz").await;
    let (client, paired) = host.pair("authz test").await;
    let device_id = paired.device_id.clone();
    let pane = "w1:p1";

    // A pending device holds nothing (D3): acting and managing are 403, and the
    // answer is uniform.
    let forbidden = client.respond(pane, "hola").await.expect_err("respond");
    assert_eq!(forbidden.status, 403, "{}", forbidden.body);
    assert!(forbidden.body.contains("forbidden"), "{}", forbidden.body);
    assert_eq!(client.devices().await.expect_err("devices").status, 403);
    assert_eq!(
        client
            .keys(pane, &["enter"], false)
            .await
            .expect_err("keys")
            .status,
        403
    );

    host.grant(&device_id, Permission::Respond);
    host.grant(&device_id, Permission::SendKey);
    host.grant(&device_id, Permission::Admin);

    // Typed action errors.
    assert_action(client.respond(pane, "").await, "empty_prompt");
    assert_action(client.respond(pane, "bell\x07").await, "control_characters");
    assert_action(client.respond("claude", "hola").await, "bad_pane");
    assert_action(client.keys(pane, &[], false).await, "no_keys");
    assert_action(client.keys(pane, &["f5"], false).await, "key_not_allowed");
    assert_action(
        client.keys(pane, &["ctrl+c"], false).await,
        "confirmation_required",
    );

    // Device management: the roster carries public material only.
    let listed = client.devices().await.expect("devices");
    let devices = listed["devices"].as_array().expect("array");
    assert!(devices
        .iter()
        .any(|device| device["device_id"] == device_id.as_str()));
    assert!(
        devices
            .iter()
            .all(|device| device.get("public_key").is_none()),
        "the roster never carries public keys"
    );

    // Revoking a second device refuses it immediately.
    let (victim, victim_pair) = host.pair("victim").await;
    host.grant(&victim_pair.device_id, Permission::ReadState);
    assert!(victim.state().await.is_ok());
    let revoked = client.revoke(&victim_pair.device_id).await.expect("revoke");
    assert_eq!(revoked["revoked"], true);
    assert_eq!(victim.state().await.expect_err("revoked").status, 401);
    assert_eq!(
        client
            .revoke("dev-missing")
            .await
            .expect_err("missing")
            .status,
        404
    );

    // An oversized signed body is refused before any parsing.
    let huge = "x".repeat(300 * 1024);
    let body = serde_json::to_vec(&serde_json::json!({ "text": huge })).expect("json");
    let (status, _) = client
        .request("POST", &format!("/v1/agents/{pane}/respond"), Some(body))
        .await
        .expect("request");
    assert_eq!(status, 413);

    // JSON that is not the expected model is a 422; not-JSON is a 400.
    let (status, body) = client
        .request(
            "POST",
            &format!("/v1/agents/{pane}/respond"),
            Some(br#"{"text": 42}"#.to_vec()),
        )
        .await
        .expect("request");
    assert_eq!(status, 422, "{body}");
    assert!(body.contains("invalid_body"), "{body}");
    let (status, body) = client
        .request(
            "POST",
            &format!("/v1/agents/{pane}/respond"),
            Some(b"not json".to_vec()),
        )
        .await
        .expect("request");
    assert_eq!(status, 400, "{body}");
    assert!(body.contains("invalid_body"), "{body}");
}

fn assert_action(outcome: Result<serde_json::Value, HttpError>, code: &str) {
    let error = outcome.expect_err("must fail");
    assert_eq!(error.status, 400, "{}", error.body);
    assert!(
        error.body.contains(&format!("\"code\":\"{code}\"")),
        "expected {code}: {}",
        error.body
    );
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

/// A signed request with every part under the caller's control, so each
/// rejection path can be exercised in isolation. An empty signature message
/// means "sign the correct canonical string".
async fn signed_raw(
    host: &TestHost,
    identity: &DemoIdentity,
    method: &str,
    path: &str,
    body: &[u8],
    timestamp: Option<i64>,
    nonce_and_message: Option<(String, String)>,
) -> (u16, String) {
    let timestamp = timestamp.unwrap_or_else(now);
    let (nonce, message) = match nonce_and_message {
        Some((nonce, message)) if !message.is_empty() => (nonce, message),
        Some((nonce, _)) => (
            nonce.clone(),
            canonical_string(method, path, body, timestamp, &nonce).expect("canonical"),
        ),
        None => {
            let nonce = format!("bm9uY2U-{timestamp}");
            let message =
                canonical_string(method, path, body, timestamp, &nonce).expect("canonical");
            (nonce, message)
        }
    };
    let signature = identity.sign(&message);
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\
         X-HN-Device: {}\r\nX-HN-Timestamp: {timestamp}\r\nX-HN-Nonce: {nonce}\r\n\
         X-HN-Signature: {signature}\r\n",
        identity.device_id()
    );
    if !body.is_empty() {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    write_and_read(host, request.as_bytes(), body).await
}

async fn raw_request(
    host: &TestHost,
    method: &str,
    path: &str,
    body: Option<Vec<u8>>,
) -> (u16, String) {
    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    if let Some(body) = &body {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    write_and_read(
        host,
        request.as_bytes(),
        body.as_deref().unwrap_or_default(),
    )
    .await
}

async fn write_and_read(host: &TestHost, head: &[u8], body: &[u8]) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(host.addr)
        .await
        .expect("connect");
    stream.write_all(head).await.expect("write");
    if !body.is_empty() {
        stream.write_all(body).await.expect("write body");
    }
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.expect("read");
    let text = String::from_utf8_lossy(&response).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("status");
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    (status, body)
}
