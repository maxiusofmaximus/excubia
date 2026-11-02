mod common;

use common::spawn_server;
use herdr_desktop_core::identity::DeviceIdentity;
use herdr_desktop_core::mail::{LoginStatus, MailFailure, SendPhase, SendRequest};
use herdr_desktop_core::HostClient;

const ACCOUNTS: &str = r#"{"ok":true,"accounts":[{"label":"trabajo","tenant":"organizations","upn":"ana@ejemplo.invalid","send_enabled":true}]}"#;
const DRAFT: &str = r#"{"ok":true,"draft":{"account":"trabajo","draft_id":"drf_1","from":"ana@ejemplo.invalid","to":["bea@ejemplo.invalid"],"cc":[],"subject":"Presupuesto","body":"Hola Bea","content_type":"text","attachments":[{"name":"presupuesto.pdf","size":12034,"sha256":"ab"}]}}"#;
const APPROVAL_LIST: &str = r#"{"ok":true,"approvals":[{"id":"apr_1","status":"approved","tool":"mail.send","summary":"Enviar Presupuesto","level":"user","details":{"account":"trabajo","from":"ana@ejemplo.invalid","to":["bea@ejemplo.invalid"],"cc":[],"subject":"Presupuesto","body_preview":"Hola Bea","attachments":[{"name":"presupuesto.pdf","size":12034,"sha256":"ab"}]}}]}"#;

fn client(base_url: &str) -> HostClient {
    HostClient::new(base_url, DeviceIdentity::generate("dev-test", "test")).unwrap()
}

fn json_body(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body).expect("the request body is JSON")
}

#[tokio::test]
async fn accounts_show_the_label_the_tenant_and_the_send_consent() {
    let (base_url, captured) = spawn_server(vec![(200, ACCOUNTS)]).await;

    let accounts = client(&base_url).mail_accounts().await.unwrap();

    assert_eq!(accounts[0].label, "trabajo");
    assert!(accounts[0].send_enabled);
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].method, "GET");
    assert_eq!(captured[0].target, "/v1/mail/accounts");
}

#[tokio::test]
async fn a_sign_in_sends_only_the_label_and_the_tenant() {
    let reply = r#"{"ok":true,"login_id":"lg_1","user_code":"ABCD-1234","verification_uri":"https://microsoft.com/devicelogin","expires_in":900,"interval":5}"#;
    let (base_url, captured) = spawn_server(vec![(200, reply)]).await;

    let login = client(&base_url)
        .mail_login_start("trabajo", "organizations")
        .await
        .unwrap();

    assert_eq!(login.user_code, "ABCD-1234");
    assert_eq!(login.expires_in, 900);
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].target, "/v1/mail/login/start");
    assert_eq!(
        json_body(&captured[0].body),
        serde_json::json!({"account": "trabajo", "tenant": "organizations"})
    );
}

#[tokio::test]
async fn an_unknown_tenant_is_refused_before_any_request() {
    let (base_url, captured) = spawn_server(vec![]).await;

    let error = client(&base_url)
        .mail_login_start("trabajo", "common")
        .await
        .unwrap_err();

    assert_eq!(error.code, "mail_invalid_input");
    assert!(captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_label_with_a_control_character_never_leaves_the_client() {
    let (base_url, captured) = spawn_server(vec![]).await;

    let error = client(&base_url)
        .mail_login_status("tra\nbajo")
        .await
        .unwrap_err();

    assert_eq!(error.code, "mail_invalid_input");
    assert!(captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_edit_sends_the_subject_and_the_body_and_returns_the_draft() {
    let reply = r#"{"ok":true,"draft":{"account":"trabajo","draft_id":"drf_1","subject":"Nuevo","body":"Hola","content_type":"text"}}"#;
    let (base_url, captured) = spawn_server(vec![(200, reply)]).await;

    let draft = client(&base_url)
        .mail_draft_update("trabajo", "drf_1", "Nuevo", "Hola")
        .await
        .unwrap();

    assert_eq!(draft.subject, "Nuevo");
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].target, "/v1/mail/drafts/drf_1/update");
    assert_eq!(
        json_body(&captured[0].body),
        serde_json::json!({"account": "trabajo", "subject": "Nuevo", "body": "Hola"})
    );
}

#[tokio::test]
async fn the_login_status_moves_from_pending_to_ok() {
    let (base_url, captured) = spawn_server(vec![
        (200, r#"{"ok":true,"login":{"status":"pending"}}"#),
        (
            200,
            r#"{"ok":true,"login":{"status":"ok","label":"trabajo","upn":"ana@ejemplo.invalid"}}"#,
        ),
    ])
    .await;
    let client = client(&base_url);

    assert!(matches!(
        client.mail_login_status("trabajo").await.unwrap(),
        LoginStatus::Pending
    ));
    assert!(matches!(
        client.mail_login_status("trabajo").await.unwrap(),
        LoginStatus::Ok { .. }
    ));
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].target, "/v1/mail/login/status?account=trabajo");
}

#[tokio::test]
async fn enabling_send_uses_its_own_sign_in_and_status() {
    let reply = r#"{"ok":true,"login_id":"lg_2","user_code":"WXYZ-9876","verification_uri":"https://microsoft.com/devicelogin","expires_in":900,"interval":5}"#;
    let (base_url, captured) = spawn_server(vec![
        (200, reply),
        (200, r#"{"ok":true,"login":{"status":"denied"}}"#),
    ])
    .await;
    let client = client(&base_url);

    client.mail_send_enable("trabajo").await.unwrap();
    let status = client.mail_send_enable_status("trabajo").await.unwrap();

    assert!(
        matches!(&status, LoginStatus::Denied { message } if message.contains("inicio de sesión"))
    );
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].target, "/v1/mail/send/enable");
    assert_eq!(
        captured[1].target,
        "/v1/mail/send/enable/status?account=trabajo"
    );
}

#[tokio::test]
async fn disabling_send_reports_no_consent() {
    let (base_url, _) = spawn_server(vec![(
        200,
        r#"{"ok":true,"account":"trabajo","send_enabled":false}"#,
    )])
    .await;

    let toggled = client(&base_url)
        .mail_send_disable("trabajo")
        .await
        .unwrap();

    assert!(!toggled.send_enabled);
}

#[tokio::test]
async fn the_draft_comes_with_its_body_and_its_attachments() {
    let (base_url, captured) = spawn_server(vec![(200, DRAFT)]).await;

    let draft = client(&base_url)
        .mail_draft("trabajo", "drf_1")
        .await
        .unwrap();

    assert_eq!(draft.body, "Hola Bea");
    assert_eq!(draft.attachments[0].name, "presupuesto.pdf");
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].target, "/v1/mail/drafts/drf_1?account=trabajo");
}

#[tokio::test]
async fn asking_to_send_without_an_approval_sends_no_approval_id() {
    let reply = r#"{"ok":true,"state":"awaiting_approval","approval_id":"apr_1","expires_in":120}"#;
    let (base_url, captured) = spawn_server(vec![(202, reply)]).await;

    let request = client(&base_url)
        .mail_draft_send("trabajo", "drf_1", None)
        .await
        .unwrap();

    assert!(matches!(
        request,
        SendRequest::AwaitingApproval {
            expires_in: 120,
            ..
        }
    ));
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].target, "/v1/mail/drafts/drf_1/send");
    assert_eq!(
        json_body(&captured[0].body),
        serde_json::json!({"account": "trabajo"})
    );
}

#[tokio::test]
async fn sending_with_the_approval_opens_the_ten_second_window() {
    let reply = r#"{"ok":true,"state":"pending","send_id":"snd_1","window_seconds":10}"#;
    let (base_url, captured) = spawn_server(vec![(202, reply)]).await;

    let request = client(&base_url)
        .mail_draft_send("trabajo", "drf_1", Some("apr_1"))
        .await
        .unwrap();

    assert!(matches!(
        request,
        SendRequest::Pending {
            window_seconds: 10,
            ..
        }
    ));
    let captured = captured.lock().unwrap();
    assert_eq!(
        json_body(&captured[0].body)["approval_id"],
        serde_json::json!("apr_1")
    );
}

#[tokio::test]
async fn a_send_that_is_not_enabled_says_how_to_fix_it() {
    let body = r#"{"detail":{"code":"mail_send_not_enabled","message":"x"}}"#;
    let (base_url, _) = spawn_server(vec![(400, body)]).await;

    let error: MailFailure = client(&base_url)
        .mail_draft_send("trabajo", "drf_1", None)
        .await
        .unwrap_err();

    assert_eq!(error.code, "mail_send_not_enabled");
    assert!(error.message.contains("Permitir enviar"));
}

#[tokio::test]
async fn a_refusal_without_the_permission_is_forbidden() {
    let (base_url, _) = spawn_server(vec![(403, r#"{"detail":"forbidden"}"#)]).await;

    let error = client(&base_url).mail_accounts().await.unwrap_err();

    assert_eq!(error.code, "forbidden");
}

#[tokio::test]
async fn cancelling_inside_the_window_settles_on_cancelled() {
    let (base_url, captured) = spawn_server(vec![(
        200,
        r#"{"ok":true,"send_id":"snd_1","state":"cancelled"}"#,
    )])
    .await;

    let state = client(&base_url).mail_send_cancel("snd_1").await.unwrap();

    assert_eq!(state.state, SendPhase::Cancelled);
    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].target, "/v1/mail/sends/snd_1/cancel");
}

#[tokio::test]
async fn the_send_state_reads_sent() {
    let (base_url, captured) = spawn_server(vec![(
        200,
        r#"{"ok":true,"send_id":"snd_1","state":"sent"}"#,
    )])
    .await;

    let state = client(&base_url).mail_send_state("snd_1").await.unwrap();

    assert_eq!(state.state, SendPhase::Sent);
    assert_eq!(captured.lock().unwrap()[0].target, "/v1/mail/sends/snd_1");
}

#[tokio::test]
async fn an_id_with_a_slash_never_reaches_the_path() {
    let (base_url, captured) = spawn_server(vec![]).await;

    let error = client(&base_url)
        .mail_send_state("snd_1/../accounts")
        .await
        .unwrap_err();

    assert_eq!(error.code, "mail_invalid_input");
    assert!(captured.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_approval_is_found_in_the_inbox_or_in_a_resolved_status() {
    let empty = r#"{"ok":true,"approvals":[]}"#;
    let (base_url, captured) = spawn_server(vec![
        (200, empty),
        // The four resolved lists answer in any order; each one carries the row.
        (200, APPROVAL_LIST),
        (200, APPROVAL_LIST),
        (200, APPROVAL_LIST),
        (200, APPROVAL_LIST),
    ])
    .await;

    let found = client(&base_url)
        .chat_approval_find("apr_1")
        .await
        .unwrap()
        .expect("the approval");

    assert_eq!(found.status, "approved");
    let details = found.details.expect("mail details for an operator device");
    assert_eq!(details.subject, "Presupuesto");
    assert_eq!(details.attachments[0].size, 12034);
    assert_eq!(captured.lock().unwrap().len(), 5);
}

#[tokio::test]
async fn an_approval_nobody_lists_is_not_found() {
    let empty = r#"{"ok":true,"approvals":[]}"#;
    let (base_url, _) = spawn_server(vec![
        (200, empty),
        (200, empty),
        (200, empty),
        (200, empty),
        (200, empty),
    ])
    .await;

    let found = client(&base_url).chat_approval_find("apr_9").await.unwrap();

    assert!(found.is_none());
}
