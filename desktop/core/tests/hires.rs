mod common;

use common::spawn_server;
use herdr_desktop_core::chat::{ChatFailure, HireChange};
use herdr_desktop_core::identity::DeviceIdentity;
use herdr_desktop_core::GatewayClient;

const HIRE_REPLY: &str = r#"{"ok":true,"hire":{"id":"hire_01JAB3ZQ5V8K2M9X4T6R0WQ7YD","role":"bob","area":"programacion","name":"Bob","avatar":"bob","status":"hired","created_at":"2026-10-25T10:00:00.000Z"}}"#;
const TEAM_REPLY: &str = r#"{"ok":true,"hires":[{"id":"hire_01JAB3ZQ5V8K2M9X4T6R0WQ7YD","role":"bob","area":"programacion","name":"Bob","avatar":"bob","status":"hired","created_at":"2026-10-25T10:00:00.000Z"}],"can_hire":true}"#;

fn client(base_url: &str) -> GatewayClient {
    GatewayClient::new(base_url, DeviceIdentity::generate("dev-test", "test")).unwrap()
}

fn json_body(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body).expect("the request body is JSON")
}

#[tokio::test]
async fn the_team_reads_its_hires_and_whether_this_device_can_hire() {
    let (base_url, captured) = spawn_server(vec![(200, TEAM_REPLY)]).await;

    let team = client(&base_url).team_hires().await.unwrap();

    assert!(team.can_hire);
    assert_eq!(team.hires.len(), 1);
    assert_eq!(team.hires[0].name, "Bob");
    assert_eq!(team.hires[0].avatar.as_deref(), Some("bob"));
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].method, "GET");
    assert_eq!(captured[0].target, "/v1/hires");
}

#[tokio::test]
async fn a_host_that_omits_can_hire_counts_as_no() {
    let (base_url, _) = spawn_server(vec![(200, r#"{"ok":true,"hires":[]}"#)]).await;

    let team = client(&base_url).team_hires().await.unwrap();

    assert!(!team.can_hire);
    assert!(team.hires.is_empty());
}

#[tokio::test]
async fn contracting_sends_only_the_role() {
    let (base_url, captured) = spawn_server(vec![(201, HIRE_REPLY)]).await;

    let hire = client(&base_url).team_hire("bob").await.unwrap();

    assert_eq!(hire.role, "bob");
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].target, "/v1/hires");
    assert_eq!(
        json_body(&captured[0].body),
        serde_json::json!({"role": "bob"})
    );
}

#[tokio::test]
async fn removing_a_character_sends_an_explicit_null() {
    let (base_url, captured) = spawn_server(vec![(200, HIRE_REPLY), (200, HIRE_REPLY)]).await;
    let client = client(&base_url);
    let id = "hire_01JAB3ZQ5V8K2M9X4T6R0WQ7YD";

    client
        .team_hire_change(id, &HireChange::Avatar(None))
        .await
        .unwrap();
    client
        .team_hire_change(id, &HireChange::Name("Roberto".to_string()))
        .await
        .unwrap();

    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].target, format!("/v1/hires/{id}/update"));
    assert_eq!(
        json_body(&captured[0].body),
        serde_json::json!({"avatar": null})
    );
    assert_eq!(
        json_body(&captured[1].body),
        serde_json::json!({"name": "Roberto"})
    );
}

#[tokio::test]
async fn dismissing_posts_to_the_hire_route() {
    let (base_url, captured) = spawn_server(vec![(200, HIRE_REPLY)]).await;

    client(&base_url)
        .team_hire_dismiss("hire_01JAB3ZQ5V8K2M9X4T6R0WQ7YD")
        .await
        .unwrap();

    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].method, "POST");
    assert_eq!(
        captured[0].target,
        "/v1/hires/hire_01JAB3ZQ5V8K2M9X4T6R0WQ7YD/dismiss"
    );
}

#[tokio::test]
async fn an_id_that_is_not_a_hire_id_never_leaves_the_device() {
    let (base_url, captured) = spawn_server(vec![]).await;

    let refused = client(&base_url).team_hire_dismiss("../roles").await;

    assert!(matches!(refused, Err(ChatFailure::Invalid { .. })));
    assert!(captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_invalid_name_is_refused_in_spanish_with_its_code() {
    let body = r#"{"detail":{"code":"invalid_name","field":"name","message":"x"}}"#;
    let (base_url, _) = spawn_server(vec![(422, body)]).await;

    let failure = client(&base_url)
        .team_hire_change(
            "hire_01JAB3ZQ5V8K2M9X4T6R0WQ7YD",
            &HireChange::Name(String::new()),
        )
        .await
        .unwrap_err();

    match failure {
        ChatFailure::TeamRefused { code, message } => {
            assert_eq!(code, "invalid_name");
            assert!(message.contains("entre 1 y 40 caracteres"));
        }
        other => panic!("se esperaba TeamRefused, llegó {other:?}"),
    }
}

#[tokio::test]
async fn a_full_area_names_the_limit() {
    let body = r#"{"detail":{"code":"area_full","message":"x"}}"#;
    let (base_url, _) = spawn_server(vec![(409, body)]).await;

    let failure = client(&base_url).team_hire("elif").await.unwrap_err();

    match failure {
        ChatFailure::TeamRefused { code, message } => {
            assert_eq!(code, "area_full");
            assert!(message.contains("ya tiene 8 agentes"));
        }
        other => panic!("se esperaba TeamRefused, llegó {other:?}"),
    }
}

#[tokio::test]
async fn an_unknown_hire_is_a_refusal_not_a_gateway_error() {
    let body = r#"{"detail":{"code":"no_such_hire","message":"x"}}"#;
    let (base_url, _) = spawn_server(vec![(404, body)]).await;

    let failure = client(&base_url)
        .team_hire_dismiss("hire_01JAB3ZQ5V8K2M9X4T6R0WQ7YD")
        .await
        .unwrap_err();

    assert!(matches!(
        failure,
        ChatFailure::TeamRefused { ref code, .. } if code == "no_such_hire"
    ));
}

#[tokio::test]
async fn a_device_without_approve_gets_the_grant_refusal() {
    let (base_url, _) = spawn_server(vec![(403, r#"{"detail":"forbidden"}"#)]).await;

    let failure = client(&base_url).team_hire("bob").await.unwrap_err();

    assert!(matches!(failure, ChatFailure::Forbidden { .. }));
}

#[tokio::test]
async fn the_onboarding_reads_whether_it_is_done_and_whether_to_offer_hires() {
    let (base_url, _) = spawn_server(vec![(
        200,
        r#"{"ok":true,"done":false,"areas":[],"can_hire":true}"#,
    )])
    .await;

    let onboarding = client(&base_url).team_onboarding().await.unwrap();

    assert!(!onboarding.done);
    assert!(onboarding.can_hire);
}

#[tokio::test]
async fn skipping_the_onboarding_sends_only_the_skip() {
    let (base_url, captured) =
        spawn_server(vec![(200, r#"{"ok":true,"done":true,"hires":[]}"#)]).await;

    let done = client(&base_url).team_onboarding_skip().await.unwrap();

    assert!(done.done);
    assert!(done.hires.is_empty());
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].target, "/v1/onboarding");
    assert_eq!(
        json_body(&captured[0].body),
        serde_json::json!({"skip": true})
    );
}

#[tokio::test]
async fn contracting_the_onboarding_sends_its_areas_and_roles() {
    let (base_url, captured) =
        spawn_server(vec![(200, r#"{"ok":true,"done":true,"hires":[]}"#)]).await;
    let areas = vec!["video".to_string()];
    let roles = vec!["vera".to_string()];

    client(&base_url)
        .team_onboarding_contract(&areas, &roles)
        .await
        .unwrap();

    let captured = captured.lock().unwrap();
    assert_eq!(
        json_body(&captured[0].body),
        serde_json::json!({"areas": ["video"], "roles": ["vera"]})
    );
}

#[tokio::test]
async fn the_catalogue_keeps_its_levels_and_areas() {
    let (base_url, _) = spawn_server(vec![(
        200,
        r#"{"ok":true,"version":1,"areas":[{"id":"video","label":"Vídeo y postproducción","roles":["vera","jade"],"levels":{"personal":["vera"],"profesional":["vera","jade"],"equipo":["vera","jade"]}}],"roles":[{"id":"vera","name":"Vera","title":"Editor de vídeo","area":"video","avatar":null}]}"#,
    )])
    .await;

    let catalog = client(&base_url).team_roles().await.unwrap();

    assert_eq!(catalog.areas[0].levels.personal, vec!["vera".to_string()]);
    assert_eq!(catalog.roles[0].title, "Editor de vídeo");
    assert_eq!(catalog.roles[0].avatar, None);
}
