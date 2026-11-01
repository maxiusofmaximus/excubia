use super::*;

fn status(status: u16, code: Option<&str>, message: &str) -> CoreError {
    CoreError::Status {
        status,
        code: code.map(str::to_string),
        field: None,
        message: message.to_string(),
    }
}

fn invalid_field(field: &str) -> CoreError {
    CoreError::Status {
        status: 422,
        code: Some("invalid_body".to_string()),
        field: Some(field.to_string()),
        message: "campo inválido".to_string(),
    }
}

#[test]
fn a_403_names_the_missing_grant_and_the_host_command() {
    let info = CoreError::Forbidden.describe(WriteAction::Respond);
    assert!(info.message.contains("RESPOND"), "{}", info.message);
    assert_eq!(
        info.action.as_deref(),
        Some(format!("Concédelo en el host: {GRANT_HINT}").as_str())
    );

    let keys = CoreError::Forbidden.describe(WriteAction::Keys);
    assert!(keys.message.contains("SEND_KEY"));
    assert!(keys.action.as_deref().unwrap().contains(GRANT_HINT));

    let read = CoreError::Forbidden.describe(WriteAction::Read);
    assert!(read.message.contains("READ_STATE"));

    let stop = CoreError::Forbidden.describe(WriteAction::Stop);
    assert!(stop.message.contains("RESPOND"));
    assert!(stop.action.as_deref().unwrap().contains(GRANT_HINT));
}

#[test]
fn the_canonical_grant_hint_is_the_core_one() {
    assert_eq!(
        GRANT_HINT,
        "host devices grant <device_id> --permission <read_state|respond|send_key|approve|admin> [--data-dir DIR]"
    );
}

#[test]
fn a_409_on_stop_explains_the_agent_is_not_running() {
    let info = status(409, None, "no running agent").describe(WriteAction::Stop);
    assert!(
        info.message.contains("No se puede detener"),
        "{}",
        info.message
    );
    assert!(info
        .action
        .as_deref()
        .unwrap()
        .contains("nueva conversación"));
}

#[test]
fn a_409_for_a_dead_pane_says_so_and_offers_an_action() {
    let info = status(409, None, "agent.respond: agent target w1:p1 not found")
        .describe(WriteAction::Respond);
    assert_eq!(info.message, "Ese panel ya no existe.");
    assert!(info.action.is_some());
}

#[test]
fn pane_gone_is_the_same_dead_panel_as_a_409() {
    let info = status(400, Some("pane_gone"), "pane w1:p1 is gone").describe(WriteAction::Respond);
    assert_eq!(info.message, "Ese panel ya no existe.");
    assert!(info.action.is_some());
}

#[test]
fn a_core_bad_pane_for_a_dead_pane_is_a_dead_panel() {
    let info = status(400, Some("bad_pane"), "pane w1:p1 is no longer known")
        .describe(WriteAction::Respond);
    assert_eq!(info.message, "Ese panel ya no existe.");
}

#[test]
fn a_malformed_pane_stays_a_bad_pane() {
    let info =
        status(400, Some("bad_pane"), "\"claude\" is not a pane id").describe(WriteAction::Respond);
    assert_eq!(info.message, "Ese panel no es válido.");
    assert!(info.action.is_some());
}

#[test]
fn other_409s_are_refusals_not_faults() {
    let info = status(409, None, "agent is no longer blocked").describe(WriteAction::Keys);
    assert!(info.message.contains("no aceptó la acción"));
    assert!(info.action.is_some());
}

#[test]
fn a_429_asks_to_wait() {
    let info = status(429, None, "slow down").describe(WriteAction::Respond);
    assert!(info.message.contains("limitando"));
    assert!(info.action.as_deref().unwrap().contains("Espera"));
}

#[test]
fn action_codes_become_spanish_text() {
    let empty =
        status(400, Some("empty_prompt"), "text is required").describe(WriteAction::Respond);
    assert_eq!(empty.message, "El mensaje está vacío.");

    let key =
        status(400, Some("key_not_allowed"), "x is not an allowed key").describe(WriteAction::Keys);
    assert_eq!(key.message, "Esa tecla no está permitida.");

    let confirmation =
        status(400, Some("confirmation_required"), "needs confirm").describe(WriteAction::Keys);
    assert!(confirmation.message.contains("confirmación"));
}

#[test]
fn a_422_names_the_field_that_failed() {
    let agent = invalid_field("agent").describe(WriteAction::CreateWorkspace);
    assert!(agent.message.contains("catálogo"), "{}", agent.message);
    assert!(agent.action.is_some());

    let cwd = invalid_field("cwd").describe(WriteAction::CreateWorkspace);
    assert!(cwd.message.contains("carpeta"));

    let label = invalid_field("label").describe(WriteAction::CreateWorkspace);
    assert!(label.message.contains("etiqueta"));
}

#[test]
fn creating_a_conversation_maps_403_and_409_with_its_own_text() {
    let forbidden = CoreError::Forbidden.describe(WriteAction::CreateWorkspace);
    assert!(forbidden.message.contains("RESPOND"));
    assert!(forbidden.action.as_deref().unwrap().contains(GRANT_HINT));

    let refused = status(409, None, "command not found").describe(WriteAction::CreateWorkspace);
    assert!(refused.message.contains("no pudo iniciar"));
    assert!(refused.action.as_deref().unwrap().contains("agents.toml"));
}

#[test]
fn a_dead_network_and_a_dead_pairing_read_differently() {
    let offline =
        CoreError::Transport("connection refused".to_string()).describe(WriteAction::Respond);
    assert!(offline.message.contains("Sin conexión"));

    let revoked = CoreError::Unauthorized.describe(WriteAction::Respond);
    assert!(revoked.message.contains("ya no confía"));
    assert!(revoked.action.as_deref().unwrap().contains("emparejarlo"));
}

#[test]
fn info_text_joins_the_message_and_the_action() {
    let info = CoreError::Forbidden.describe(WriteAction::Respond);
    let text = info.text();
    assert!(text.starts_with("Este equipo solo puede leer"));
    assert!(text.contains(GRANT_HINT), "{text}");
    assert!(!text.contains("herdr-notify devices"), "{text}");
}

#[test]
fn a_pin_mismatch_tells_the_user_not_to_connect() {
    assert_eq!(
        CoreError::PinMismatch.pairing_message(),
        "La huella del host no coincide. No te conectes a esta red. Si el host cambió, genera un enlace nuevo."
    );
    assert_eq!(
        CoreError::LinkExpired.pairing_message(),
        "El enlace de emparejamiento ha caducado. Genera uno nuevo en el host."
    );
}
