mod support;

use std::time::Duration;

use hn_core_mail::{
    assert_scopes_safe, assert_send_scopes_safe, poll_device_login, refresh, refresh_send,
    start_device_login, start_send_consent, MailError, Tenant, SCOPES, SEND_SCOPES,
};
use support::mock::{MockResponse, MockTransport};
use support::{device_code_body, token_body, ACCESS_TOKEN, CLIENT_ID, DEVICE_CODE, REFRESH_TOKEN};

#[tokio::test(start_paused = true)]
async fn the_happy_login_shows_the_code_and_never_asks_for_mail_send() {
    let transport = MockTransport::new(vec![
        MockResponse::ok(device_code_body()),
        MockResponse::ok(token_body(ACCESS_TOKEN, Some(REFRESH_TOKEN))),
    ]);

    let login = start_device_login(&transport, CLIENT_ID, "trabajo", Tenant::Organizations)
        .await
        .expect("device code");
    assert_eq!(login.label, "trabajo");
    assert_eq!(login.user_code, "ABCD-EFGH");
    assert_eq!(login.verification_uri, "https://microsoft.com/devicelogin");
    assert_eq!(login.expires_in, 900);
    assert_eq!(login.interval, 5);
    assert!(!format!("{login:?}").contains(DEVICE_CODE));

    let tokens = poll_device_login(&transport, CLIENT_ID, Tenant::Organizations, &login)
        .await
        .expect("tokens");
    assert_eq!(tokens.access_token(), ACCESS_TOKEN);
    assert_eq!(tokens.refresh_token(), Some(REFRESH_TOKEN));
    assert_eq!(tokens.expires_in(), 3600);

    let calls = transport.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0].url,
        "https://login.microsoftonline.com/organizations/oauth2/v2.0/devicecode"
    );
    assert_eq!(calls[0].form_value("client_id"), Some(CLIENT_ID));
    assert_eq!(
        calls[0].form_value("scope"),
        Some("offline_access User.Read Mail.ReadWrite")
    );
    assert_eq!(
        calls[1].url,
        "https://login.microsoftonline.com/organizations/oauth2/v2.0/token"
    );
    assert_eq!(
        calls[1].form_value("grant_type"),
        Some("urn:ietf:params:oauth:grant-type:device_code")
    );
    assert!(calls[0].bearer.is_none());
    assert!(calls[1].bearer.is_none());
    for call in &calls {
        assert!(
            !call.text().contains("Mail.Send"),
            "forbidden scope in {call:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn pending_then_slow_down_then_success_respects_the_interval() {
    let transport = MockTransport::new(vec![
        MockResponse::ok(device_code_body()),
        MockResponse::status(400, r#"{"error":"authorization_pending"}"#),
        MockResponse::status(400, r#"{"error":"slow_down"}"#),
        MockResponse::ok(token_body(ACCESS_TOKEN, Some(REFRESH_TOKEN))),
    ]);

    let login = start_device_login(&transport, CLIENT_ID, "trabajo", Tenant::Consumers)
        .await
        .expect("device code");
    let started = tokio::time::Instant::now();
    let tokens = poll_device_login(&transport, CLIENT_ID, Tenant::Consumers, &login)
        .await
        .expect("tokens");
    assert_eq!(tokens.access_token(), ACCESS_TOKEN);

    assert_eq!(transport.calls().len(), 4);
    assert!(
        started.elapsed() >= Duration::from_secs(15),
        "interval 5 plus slow_down 10 must elapse, got {:?}",
        started.elapsed()
    );
}

#[tokio::test(start_paused = true)]
async fn an_expired_code_is_terminal() {
    let transport = MockTransport::new(vec![
        MockResponse::ok(device_code_body()),
        MockResponse::status(400, r#"{"error":"expired_token"}"#),
    ]);
    let login = start_device_login(&transport, CLIENT_ID, "trabajo", Tenant::Common)
        .await
        .expect("device code");
    let error = poll_device_login(&transport, CLIENT_ID, Tenant::Common, &login)
        .await
        .expect_err("expired");
    assert!(matches!(error, MailError::LoginExpired));
    assert_eq!(error.code(), "mail_login_expired");
}

#[tokio::test(start_paused = true)]
async fn a_denied_sign_in_is_terminal() {
    let transport = MockTransport::new(vec![
        MockResponse::ok(device_code_body()),
        MockResponse::status(400, r#"{"error":"access_denied"}"#),
    ]);
    let login = start_device_login(&transport, CLIENT_ID, "personal", Tenant::Consumers)
        .await
        .expect("device code");
    let error = poll_device_login(&transport, CLIENT_ID, Tenant::Consumers, &login)
        .await
        .expect_err("denied");
    assert!(matches!(error, MailError::LoginDenied));
    assert_eq!(error.code(), "mail_login_denied");
}

#[tokio::test(start_paused = true)]
async fn the_login_stops_at_the_lifetime_of_the_code() {
    let short_code = device_code_body().replace("\"expires_in\":900", "\"expires_in\":10");
    let transport = MockTransport::new(vec![
        MockResponse::ok(short_code),
        MockResponse::status(400, r#"{"error":"authorization_pending"}"#),
        MockResponse::status(400, r#"{"error":"authorization_pending"}"#),
        MockResponse::status(400, r#"{"error":"authorization_pending"}"#),
        MockResponse::status(400, r#"{"error":"authorization_pending"}"#),
    ]);
    let login = start_device_login(&transport, CLIENT_ID, "trabajo", Tenant::Organizations)
        .await
        .expect("device code");
    let error = poll_device_login(&transport, CLIENT_ID, Tenant::Organizations, &login)
        .await
        .expect_err("lifetime");
    assert!(matches!(error, MailError::LoginExpired));
    assert!(transport.calls().len() <= 4);
}

#[tokio::test(start_paused = true)]
async fn refresh_rotates_tokens_and_keeps_the_old_one_when_microsoft_does_not() {
    let transport = MockTransport::new(vec![
        MockResponse::ok(token_body("access-2", None)),
        MockResponse::ok(token_body("access-3", Some("refresh-3"))),
    ]);

    let kept = refresh(&transport, CLIENT_ID, Tenant::Organizations, REFRESH_TOKEN)
        .await
        .expect("refresh");
    assert_eq!(kept.access_token(), "access-2");
    assert_eq!(kept.refresh_token(), Some(REFRESH_TOKEN));

    let rotated = refresh(&transport, CLIENT_ID, Tenant::Organizations, REFRESH_TOKEN)
        .await
        .expect("refresh again");
    assert_eq!(rotated.access_token(), "access-3");
    assert_eq!(rotated.refresh_token(), Some("refresh-3"));

    let calls = transport.calls();
    assert_eq!(calls[0].form_value("grant_type"), Some("refresh_token"));
    assert_eq!(calls[0].form_value("refresh_token"), Some(REFRESH_TOKEN));
    assert_eq!(
        calls[0].form_value("scope"),
        Some("offline_access User.Read Mail.ReadWrite")
    );
    assert!(!calls[0].text().contains("Mail.Send"));
}

#[tokio::test(start_paused = true)]
async fn aad_consent_and_missing_app_become_clear_errors_without_secrets() {
    let description = format!(
        "AADSTS65001: The user or administrator has not consented. access_token={ACCESS_TOKEN}"
    );
    let transport = MockTransport::new(vec![MockResponse::status(
        400,
        serde_json::json!({"error": "invalid_grant", "error_description": description}).to_string(),
    )]);
    let error = refresh(&transport, CLIENT_ID, Tenant::Organizations, REFRESH_TOKEN)
        .await
        .expect_err("consent");
    assert!(matches!(error, MailError::AdminConsentRequired { .. }));
    assert_eq!(error.code(), "mail_admin_consent_required");
    assert!(format!("{error}").contains("administrator"));
    assert!(!format!("{error}").contains(ACCESS_TOKEN));
    assert!(!format!("{error:?}").contains(ACCESS_TOKEN));

    let transport = MockTransport::new(vec![MockResponse::status(
        400,
        r#"{"error":"unauthorized_client","error_description":"AADSTS700016: Application with identifier 'x' was not found in the directory"}"#,
    )]);
    let error = refresh(&transport, CLIENT_ID, Tenant::Organizations, REFRESH_TOKEN)
        .await
        .expect_err("missing app");
    assert!(matches!(error, MailError::AppNotRegistered { .. }));
    assert_eq!(error.code(), "mail_app_not_registered");
    assert!(format!("{error}").contains("client_id"));
}

#[tokio::test(start_paused = true)]
async fn an_unknown_aad_failure_is_reported_as_auth_failed() {
    let transport = MockTransport::new(vec![MockResponse::status(
        400,
        r#"{"error":"invalid_grant","error_description":"AADSTS70008: The refresh token has expired"}"#,
    )]);
    let error = refresh(&transport, CLIENT_ID, Tenant::Consumers, REFRESH_TOKEN)
        .await
        .expect_err("expired refresh");
    assert!(matches!(error, MailError::AuthRejected { .. }));
    assert_eq!(error.code(), "mail_auth_failed");
}

#[test]
fn the_scope_list_is_closed_and_never_contains_mail_send() {
    assert!(assert_scopes_safe().is_ok());
    assert_eq!(SCOPES, ["offline_access", "User.Read", "Mail.ReadWrite"]);
    assert!(!SCOPES
        .iter()
        .any(|scope| scope.to_ascii_lowercase().contains("mail.send")));
}

#[test]
fn the_send_scope_list_is_exactly_offline_access_and_mail_send() {
    assert!(assert_send_scopes_safe().is_ok());
    assert_eq!(SEND_SCOPES, ["offline_access", "Mail.Send"]);
    assert!(!SEND_SCOPES
        .iter()
        .any(|scope| scope.to_ascii_lowercase().contains("read")));
}

#[tokio::test]
async fn the_send_consent_asks_for_mail_send_and_nothing_that_reads_mail() {
    let transport = MockTransport::new(vec![MockResponse::ok(device_code_body())]);

    let login = start_send_consent(&transport, CLIENT_ID, "trabajo", Tenant::Organizations)
        .await
        .expect("consent");
    assert_eq!(login.label, "trabajo");

    let calls = transport.calls();
    assert_eq!(
        calls[0].form_value("scope"),
        Some("offline_access Mail.Send")
    );
}

#[tokio::test]
async fn refreshing_the_send_tokens_asks_for_the_send_scopes_only() {
    let transport = MockTransport::new(vec![MockResponse::ok(token_body(
        ACCESS_TOKEN,
        Some(REFRESH_TOKEN),
    ))]);

    refresh_send(&transport, CLIENT_ID, Tenant::Organizations, REFRESH_TOKEN)
        .await
        .expect("refresh");

    let calls = transport.calls();
    assert_eq!(
        calls[0].form_value("scope"),
        Some("offline_access Mail.Send")
    );
}

#[test]
fn tenants_are_the_three_allowed_authorities() {
    assert_eq!(Tenant::Consumers.as_str(), "consumers");
    assert_eq!(Tenant::Organizations.as_str(), "organizations");
    assert_eq!(Tenant::Common.as_str(), "common");
    assert_eq!(
        Tenant::parse("CONSUMERS").expect("tenant"),
        Tenant::Consumers
    );
    let error = Tenant::parse("sena").expect_err("unknown");
    assert_eq!(error.code(), "mail_invalid_config");
}
