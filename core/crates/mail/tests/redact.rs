mod support;

use excubia_mail::{redact, MailError, Redacted, Secret};

const TOKEN: &str =
    "eyJ0eXAiOiJKV1QiLCJhbGciOiJSUzI1NiJ9.eyJhdWQiOiJleGN1YmlhIn0.c2lnbmF0dXJlLWxvbmctZW5vdWdo";
const UPLOAD_URL: &str = "https://outlook.office.com/upload/session?tempauth=abc123secret";

#[test]
fn a_keyed_access_token_disappears() {
    let text = format!("Graph said: access_token={TOKEN} is invalid");
    let scrubbed = redact(&text);
    assert!(!scrubbed.contains(TOKEN));
    assert!(scrubbed.contains("<redacted>"));
    assert!(scrubbed.contains("Graph said"));
}

#[test]
fn an_authorization_header_disappears() {
    let text = format!("Authorization: Bearer {TOKEN}");
    let scrubbed = redact(&text);
    assert!(!scrubbed.contains(TOKEN));
    assert!(!scrubbed.contains("Bearer "));
}

#[test]
fn an_upload_url_disappears() {
    let text = format!("the session at uploadUrl=\"{UPLOAD_URL}\" expired");
    let scrubbed = redact(&text);
    assert!(!scrubbed.contains(UPLOAD_URL));
    assert!(!scrubbed.contains("tempauth"));
    assert!(scrubbed.contains("expired"));
}

#[test]
fn json_shaped_keys_are_redacted_too() {
    let text = format!(
        r#"{{"access_token":"{TOKEN}","uploadUrl":"{UPLOAD_URL}","refresh_token":"{TOKEN}"}}"#
    );
    let scrubbed = redact(&text);
    assert!(!scrubbed.contains(TOKEN));
    assert!(!scrubbed.contains(UPLOAD_URL));
    assert!(scrubbed.contains("\"access_token\":\"<redacted>\""));
    assert!(scrubbed.contains("\"uploadUrl\":\"<redacted>\""));
}

#[test]
fn a_bare_base64url_run_is_removed() {
    let bare = "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8S9t0U1v2W3x4Y5z6";
    let text = format!("token {bare} rejected");
    let scrubbed = redact(&text);
    assert!(!scrubbed.contains(bare));
    assert!(scrubbed.contains("token"));
    assert!(scrubbed.contains("rejected"));
}

#[test]
fn ordinary_text_survives_the_redactor() {
    let text = "draft 123 for ana@example.com was created in 2 seconds";
    assert_eq!(redact(text), text);
}

#[test]
fn errors_never_print_a_token_in_display_or_debug() {
    let error = MailError::Graph {
        status: 400,
        code: "BadRequest".to_owned(),
        message: Redacted::new(format!("invalid access_token={TOKEN}")),
    };
    assert!(!format!("{error}").contains(TOKEN));
    assert!(!format!("{error:?}").contains(TOKEN));

    let raw = MailError::Transport(format!("uploadUrl={UPLOAD_URL}").into());
    assert!(!format!("{raw}").contains(UPLOAD_URL));
    assert!(!format!("{raw:?}").contains(UPLOAD_URL));
}

#[test]
fn a_secret_prints_only_its_marker() {
    let secret = Secret::new(TOKEN);
    assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
    assert_eq!(secret.expose(), TOKEN);
}
