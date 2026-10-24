use super::*;

fn status(status: u16, code: Option<&str>, message: &str) -> ErrorInfo {
    explain_error(
        status,
        code.map(str::to_string),
        message.to_string(),
        WriteAction::Read,
    )
}

#[test]
fn a_401_is_terminal_and_asks_for_a_repair() {
    let info = status(401, None, "unauthorized");
    assert_eq!(info.kind, FailureKind::Repair);
    assert!(info.message.contains("ya no confía"));
    assert!(info.action.unwrap().contains("emparejarlo"));
}

#[test]
fn a_403_names_the_missing_grant_and_the_correct_host_command() {
    let read = status(403, None, "forbidden");
    assert_eq!(read.kind, FailureKind::Permission);
    assert!(read.message.contains("READ_STATE"));
    let command = read.action.unwrap();
    assert!(command.contains("host devices grant <device_id> --permission read_state"));
    assert!(!command.contains("herdr-notify devices"));

    let respond = explain_error(403, None, "forbidden".to_string(), WriteAction::Respond);
    assert!(respond.message.contains("RESPOND"));
    assert!(respond
        .action
        .as_deref()
        .unwrap()
        .contains("--permission respond"));

    let keys = explain_error(403, None, "forbidden".to_string(), WriteAction::Keys);
    assert!(keys.message.contains("SEND_KEY"));
    assert!(keys
        .action
        .as_deref()
        .unwrap()
        .contains("--permission send_key"));

    let seen = explain_error(403, None, "forbidden".to_string(), WriteAction::Seen);
    assert!(seen.message.contains("READ_STATE"));
    assert!(seen.message.contains("visto"));

    let admin = explain_error(403, None, "forbidden".to_string(), WriteAction::Admin);
    assert!(admin.message.contains("ADMIN"));
}

#[test]
fn pane_gone_and_a_dead_409_are_the_same_dead_panel() {
    let gone = status(400, Some("pane_gone"), "pane w1:p1 is gone");
    assert_eq!(gone.message, "Ese panel ya no existe.");
    assert!(gone.action.is_some());

    let dead = status(409, None, "agent.respond: agent target w1:p1 not found");
    assert_eq!(dead.message, "Ese panel ya no existe.");
}

#[test]
fn a_restored_pane_explains_that_the_agent_ended() {
    let info = status(409, None, "no running agent");
    assert!(info.message.contains("El agente ya terminó"));
    assert!(info
        .action
        .as_deref()
        .unwrap()
        .contains("conversación nueva"));
    assert_ne!(info.message, "Ese panel ya no existe.");
}

#[test]
fn a_malformed_pane_stays_a_bad_pane() {
    let info = status(400, Some("bad_pane"), "\"claude\" is not a pane id");
    assert_eq!(info.message, "Ese panel no es válido.");
}

#[test]
fn a_legacy_dead_pane_bad_pane_maps_to_the_dead_panel() {
    let info = status(400, Some("bad_pane"), "pane w1:p1 is no longer known");
    assert_eq!(info.message, "Ese panel ya no existe.");
}

#[test]
fn action_codes_become_text() {
    assert_eq!(
        status(400, Some("empty_prompt"), "text is required").message,
        "El mensaje está vacío."
    );
    assert_eq!(
        status(400, Some("key_not_allowed"), "x").message,
        "Esa tecla no está permitida."
    );
    assert!(status(400, Some("confirmation_required"), "x")
        .message
        .contains("confirmación"));
    assert_eq!(
        status(400, Some("image_too_large"), "x").message,
        "La imagen es demasiado grande."
    );
}

#[test]
fn a_429_asks_to_wait_and_stays_retryable() {
    let info = status(429, None, "slow down");
    assert_eq!(info.kind, FailureKind::Retry);
    assert!(info.message.contains("limitando"));
    assert!(info.action.unwrap().contains("Espera"));
}

#[test]
fn a_network_error_and_a_server_fault_read_differently() {
    let offline = status(0, None, "connection refused");
    assert!(offline.message.contains("Sin conexión"));

    let fault = status(503, None, "unavailable");
    assert!(fault.message.contains("fallo interno"));
    assert_eq!(fault.kind, FailureKind::Retry);
}

#[test]
fn pairing_errors_are_retryable_and_explained() {
    let rejected = explain_pairing_error(403);
    assert!(rejected.message.contains("código"));
    assert_eq!(rejected.kind, FailureKind::Retry);

    let limited = explain_pairing_error(429);
    assert!(limited.message.contains("Demasiados"));

    let offline = explain_pairing_error(0);
    assert!(offline.message.contains("No se pudo contactar"));

    let invalid = explain_pairing_error(200);
    assert!(invalid.message.contains("no es un emparejamiento"));
}

#[test]
fn an_http_error_body_is_parsed_before_it_is_explained() {
    let forbidden = explain_http_error(
        403,
        r#"{"detail":"forbidden"}"#.to_string(),
        WriteAction::Read,
    );
    assert_eq!(forbidden.kind, FailureKind::Permission);
    assert!(forbidden.message.contains("READ_STATE"));

    let gone = explain_http_error(
        400,
        r#"{"detail":{"code":"pane_gone","message":"pane w1:p1 is gone"}}"#.to_string(),
        WriteAction::Respond,
    );
    assert_eq!(gone.message, "Ese panel ya no existe.");

    let action_code = explain_http_error(
        400,
        r#"{"code":"empty_prompt","message":"text is required"}"#.to_string(),
        WriteAction::Respond,
    );
    assert_eq!(action_code.message, "El mensaje está vacío.");

    let raw = explain_http_error(
        502,
        "<html>bad gateway</html>".to_string(),
        WriteAction::Read,
    );
    assert!(raw.message.contains("fallo interno"));
}

#[test]
fn a_403_for_creating_or_stopping_asks_for_respond_with_the_canonical_command() {
    for action in [WriteAction::Create, WriteAction::Stop] {
        let info = explain_error(403, None, "forbidden".to_string(), action);
        assert_eq!(info.kind, FailureKind::Permission);
        assert!(info.message.contains("RESPOND"));
        assert!(info
            .action
            .as_deref()
            .unwrap()
            .contains("host devices grant <device_id> --permission respond"));
    }
}

#[test]
fn a_422_names_the_create_field_that_failed() {
    for (field, expected) in [
        ("agent", "no está en el catálogo"),
        ("cwd", "carpeta no existe"),
        ("label", "etiqueta no es válida"),
    ] {
        let body = format!(
            r#"{{"detail":{{"code":"invalid_body","errors":[{{"loc":["{field}"],"msg":"x"}}]}}}}"#
        );
        let info = explain_create_error(422, body);
        assert!(
            info.message.contains(expected),
            "field {field}: {}",
            info.message
        );
    }
}

#[test]
fn a_422_without_a_known_field_stays_generic() {
    let info = explain_create_error(
        422,
        r#"{"detail":{"code":"invalid_body","errors":[]}}"#.to_string(),
    );
    assert_eq!(info.message, "La petición no es válida.");
}

#[test]
fn creating_a_conversation_explains_a_409_and_a_429() {
    let refused = explain_create_error(409, "pty failed".to_string());
    assert!(refused.message.contains("no pudo iniciar"));

    let limited = explain_create_error(429, r#"{"detail":"slow down"}"#.to_string());
    assert!(limited.message.contains("limitando"));
}

#[test]
fn a_stop_that_finds_the_pane_gone_is_a_friendly_notice() {
    let gone = classify_stop_error(
        400,
        r#"{"detail":{"code":"pane_gone","message":"pane w1:p1 is gone"}}"#.to_string(),
    );
    assert!(gone.gone);
    assert_eq!(gone.error.message, "El agente ya había terminado.");
    assert!(gone.error.action.is_some());
}

#[test]
fn a_stop_without_permission_names_respond_and_a_restored_pane_is_ended() {
    let forbidden = classify_stop_error(403, r#"{"detail":"forbidden"}"#.to_string());
    assert!(!forbidden.gone);
    assert!(forbidden.error.message.contains("RESPOND"));
    assert!(forbidden
        .error
        .action
        .as_deref()
        .unwrap()
        .contains("--permission respond"));

    let restored = classify_stop_error(409, r#"{"detail":"no running agent"}"#.to_string());
    assert!(!restored.gone);
    assert!(restored.error.message.contains("El agente ya terminó"));
}
