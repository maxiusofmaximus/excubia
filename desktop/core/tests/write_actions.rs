mod common;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use common::{spawn_server, Captured};
use herdr_desktop_core::chat::ChatFailure;
use herdr_desktop_core::error::{CoreError, WriteAction};
use herdr_desktop_core::identity::DeviceIdentity;
use herdr_desktop_core::signing::canonical_string;
use herdr_desktop_core::HostClient;
use p256::ecdsa::{signature::Verifier, Signature};

fn header<'a>(headers: &'a str, name: &str) -> &'a str {
    headers
        .lines()
        .find_map(|line| {
            let (field, value) = line.split_once(':')?;
            field.eq_ignore_ascii_case(name).then(|| value.trim())
        })
        .unwrap_or_else(|| panic!("missing header {name}"))
}

fn verify_signature(identity: &DeviceIdentity, captured: &Captured) {
    assert_eq!(
        header(&captured.headers, "x-hn-device"),
        identity.device_id()
    );
    let timestamp: u64 = header(&captured.headers, "x-hn-timestamp")
        .parse()
        .expect("timestamp must be seconds");
    let nonce = header(&captured.headers, "x-hn-nonce");
    let canonical = canonical_string(
        &captured.method,
        &captured.target,
        &captured.body,
        timestamp,
        nonce,
    );
    let der = STANDARD
        .decode(header(&captured.headers, "x-hn-signature"))
        .expect("signature must be base64");
    let signature = Signature::from_der(&der).expect("signature must be DER");
    identity
        .verifying_key()
        .verify(canonical.as_bytes(), &signature)
        .expect("the host would verify this signature");
}

fn client(base_url: &str) -> HostClient {
    HostClient::new(base_url, DeviceIdentity::generate("dev-test", "test")).unwrap()
}

#[tokio::test]
async fn writes_sign_the_exact_contract_body() {
    let ok = r#"{"ok":true,"result":{"pane_id":"w1:p1"}}"#;
    let (base_url, captured) = spawn_server(vec![(200, ok), (200, ok), (200, ok)]).await;
    let identity = DeviceIdentity::generate("dev-test", "test");
    let client = HostClient::new(&base_url, identity.clone()).unwrap();

    client.respond("w1:p1", "hola, ¿sigues ahí?").await.unwrap();
    client
        .send_keys("w1:p1", &["enter".to_string(), "esc".to_string()], false)
        .await
        .unwrap();
    client.mark_seen("w1:p1").await.unwrap();

    let captured = captured.lock().unwrap();
    assert_eq!(captured.len(), 3);
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].target, "/v1/agents/w1:p1/respond");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&captured[0].body).unwrap(),
        serde_json::json!({"text": "hola, ¿sigues ahí?"})
    );
    assert_eq!(captured[1].target, "/v1/agents/w1:p1/keys");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&captured[1].body).unwrap(),
        serde_json::json!({"keys": ["enter", "esc"], "confirm": false})
    );
    assert_eq!(captured[2].target, "/v1/agents/w1:p1/seen");
    assert!(captured[2].body.is_empty(), "seen carries no body");

    // Signatures are checked against the public key, like the host does.
    for request in captured.iter() {
        verify_signature(&identity, request);
    }
}

#[tokio::test]
async fn a_403_on_a_write_becomes_a_spanish_grant_hint() {
    let (base_url, _) = spawn_server(vec![(403, r#"{"detail":"forbidden"}"#)]).await;
    let error = client(&base_url)
        .respond("w1:p1", "hola")
        .await
        .unwrap_err();

    assert!(matches!(error, CoreError::Forbidden));
    let info = error.describe(WriteAction::Respond);
    assert!(info.message.contains("RESPOND"));
    assert!(info
        .action
        .as_deref()
        .unwrap()
        .contains("host devices grant <device_id>"));
}

#[tokio::test]
async fn a_409_for_a_missing_pane_is_a_dead_panel() {
    let (base_url, _) = spawn_server(vec![(
        409,
        r#"{"detail":"agent.respond: agent target w1:p1 not found"}"#,
    )])
    .await;
    let error = client(&base_url)
        .respond("w1:p1", "hola")
        .await
        .unwrap_err();

    assert!(matches!(error, CoreError::Status { status: 409, .. }));
    let info = error.describe(WriteAction::Respond);
    assert_eq!(info.message, "Ese panel ya no existe.");
    assert!(info.action.is_some());
}

#[tokio::test]
async fn a_429_says_to_wait() {
    let (base_url, _) = spawn_server(vec![(429, r#"{"detail":"slow down"}"#)]).await;
    let error = client(&base_url)
        .send_keys("w1:p1", &["enter".to_string()], false)
        .await
        .unwrap_err();

    assert!(matches!(error, CoreError::Status { status: 429, .. }));
    let info = error.describe(WriteAction::Keys);
    assert!(info.message.contains("limitando"));
    assert!(info.action.as_deref().unwrap().contains("Espera"));
}

#[tokio::test]
async fn a_400_action_code_is_mapped_to_its_own_message() {
    let (base_url, _) = spawn_server(vec![(
        400,
        r#"{"detail":{"code":"empty_prompt","message":"text is required"}}"#,
    )])
    .await;
    let error = client(&base_url).respond("w1:p1", "   ").await.unwrap_err();

    match &error {
        CoreError::Status { code, .. } => assert_eq!(code.as_deref(), Some("empty_prompt")),
        other => panic!("expected a status error, got {other:?}"),
    }
    let info = error.describe(WriteAction::Respond);
    assert_eq!(info.message, "El mensaje está vacío.");
}

#[tokio::test]
async fn the_catalogue_is_a_signed_get_with_names_and_availability() {
    let body = r#"{"ok":true,"agents":[{"name":"opencode","available":true,"label":"opencode"},{"name":"claude","available":false}]}"#;
    let (base_url, captured) = spawn_server(vec![(200, body)]).await;

    let agents = client(&base_url).catalog().await.unwrap();
    assert_eq!(agents.len(), 2);
    assert_eq!(agents[0].name, "opencode");
    assert!(agents[0].available);
    assert_eq!(agents[0].label.as_deref(), Some("opencode"));
    assert!(!agents[1].available);

    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].method, "GET");
    assert_eq!(captured[0].target, "/v1/catalog");
    assert!(captured[0].body.is_empty());
}

#[tokio::test]
async fn the_catalogue_says_what_windows_can_run_and_why_not() {
    let body = r#"{"ok":true,"agents":[{"name":"codex","available":true,"runnable":false,"problem":"Windows no puede ejecutarlo tal como está instalado."},{"name":"claude","available":true,"runnable":true}]}"#;
    let (base_url, _) = spawn_server(vec![(200, body)]).await;

    let agents = client(&base_url).catalog().await.unwrap();
    assert!(!agents[0].runnable);
    assert!(agents[0].problem.is_some());
    assert!(agents[1].runnable);
}

#[tokio::test]
async fn stop_is_a_signed_empty_post_and_a_second_one_is_pane_gone() {
    let (base_url, captured) = spawn_server(vec![(200, r#"{"ok":true,"pane_id":"w1:p1"}"#)]).await;
    client(&base_url).stop_agent("w1:p1").await.unwrap();
    {
        let captured = captured.lock().unwrap();
        assert_eq!(captured[0].method, "POST");
        assert_eq!(captured[0].target, "/v1/agents/w1:p1/stop");
        assert!(captured[0].body.is_empty());
    }

    let gone = r#"{"detail":{"code":"pane_gone","message":"pane w1:p1 is gone"}}"#;
    let (base_url, _) = spawn_server(vec![(400, gone)]).await;
    let error = client(&base_url).stop_agent("w1:p1").await.unwrap_err();
    match &error {
        CoreError::Status { status, code, .. } => {
            assert_eq!(*status, 400);
            assert_eq!(code.as_deref(), Some("pane_gone"));
        }
        other => panic!("expected a status error, got {other:?}"),
    }
    assert_eq!(
        error.describe(WriteAction::Stop).message,
        "Ese panel ya no existe."
    );
}

#[tokio::test]
async fn an_invalid_pane_never_reaches_the_wire() {
    let (base_url, captured) = spawn_server(vec![]).await;
    let error = client(&base_url)
        .respond("herdr agent", "hola")
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::InvalidInput(_)));
    assert!(captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn chat_stop_is_an_empty_signed_post_and_a_second_stop_is_not_an_error() {
    let (base_url, captured) =
        spawn_server(vec![(200, r#"{"ok":true,"stopped":false,"runs":0}"#)]).await;
    let outcome = client(&base_url)
        .chat_stop_conversation("cnv_01JABC")
        .await
        .unwrap();
    assert!(!outcome.stopped);
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].target, "/v1/chat/conversations/cnv_01JABC/stop");
    assert!(captured[0].body.is_empty());
}

#[tokio::test]
async fn chat_stop_maps_a_missing_permission_and_an_old_host_apart() {
    let (base_url, _) = spawn_server(vec![(403, r#"{"detail":"forbidden"}"#)]).await;
    let error = client(&base_url)
        .chat_stop_conversation("cnv_01JABC")
        .await
        .unwrap_err();
    assert!(matches!(error, ChatFailure::Forbidden { .. }));

    let (base_url, _) = spawn_server(vec![(405, "")]).await;
    let error = client(&base_url)
        .chat_stop_conversation("cnv_01JABC")
        .await
        .unwrap_err();
    assert!(matches!(error, ChatFailure::Host { status: 405, .. }));
}
