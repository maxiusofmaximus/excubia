//! One place where an HTTP outcome becomes Spanish text plus an action. The
//! UI never branches on raw status codes; it renders what this module returns.

pub mod chat;

pub use chat::{explain_chat_error, local_chat_failure, ChatFailure};

/// How the UI should react to a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Try again from the same screen; the pairing is intact.
    Retry,
    /// The host no longer trusts this device: only a re-pair fixes it.
    Repair,
    /// The pairing is intact but a grant is missing; the fix is host-side.
    Permission,
}

/// The intent behind a request, used to name the missing grant on a 403.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteAction {
    Read,
    Respond,
    Keys,
    Seen,
    Create,
    Stop,
    Admin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorInfo {
    pub message: String,
    pub action: Option<String>,
    pub kind: FailureKind,
}

#[flutter_rust_bridge::frb(sync)]
pub fn explain_error(
    status: u16,
    code: Option<String>,
    message: String,
    action: WriteAction,
) -> ErrorInfo {
    match status {
        0 => info(
            "Sin conexión con el gateway.",
            Some("Comprueba la red y vuelve a intentarlo."),
            FailureKind::Retry,
        ),
        401 => info(
            "El gateway ya no confía en este dispositivo.",
            Some("Vuelve a emparejarlo desde Avanzado."),
            FailureKind::Repair,
        ),
        403 => forbidden(action),
        409 if looks_like_a_dead_panel(&message) => dead_panel(),
        409 if message.contains("no running agent") => ended_panel(),
        409 => info(
            format!("El agente no aceptó la acción: {message}"),
            Some("Actualiza el estado y vuelve a intentarlo."),
            FailureKind::Retry,
        ),
        400 => bad_request(code.as_deref(), &message),
        413 => info(
            "El mensaje es demasiado largo para el gateway.",
            Some("Acórtalo y vuelve a intentarlo."),
            FailureKind::Retry,
        ),
        422 => info("La petición no es válida.", None, FailureKind::Retry),
        429 => info(
            "El gateway está limitando las peticiones.",
            Some("Espera unos segundos y vuelve a intentarlo."),
            FailureKind::Retry,
        ),
        status if status >= 500 => info(
            format!("El gateway tuvo un fallo interno ({status})."),
            Some("Vuelve a intentarlo en unos segundos."),
            FailureKind::Retry,
        ),
        status => info(
            format!("El gateway respondió {status}: {message}"),
            Some("Vuelve a intentarlo."),
            FailureKind::Retry,
        ),
    }
}

/// Decode a gateway error body and explain it in one step. The body shapes are
/// `{"detail": "..."}`, `{"detail": {code, message}}` and `{code, message}`.
#[flutter_rust_bridge::frb(sync)]
pub fn explain_http_error(status: u16, body: String, action: WriteAction) -> ErrorInfo {
    let (code, message) = parse_error_body(&body);
    explain_error(status, code, message, action)
}

#[flutter_rust_bridge::frb(sync)]
pub fn explain_pairing_error(status: u16) -> ErrorInfo {
    match status {
        0 => info(
            "No se pudo contactar con el equipo.",
            Some("Comprueba la red y la dirección del gateway."),
            FailureKind::Retry,
        ),
        400 => info(
            "La petición de emparejamiento no es válida.",
            None,
            FailureKind::Retry,
        ),
        403 => info(
            "El código no es válido o ha caducado.",
            Some("Pide uno nuevo en el equipo con herdr-notify enroll."),
            FailureKind::Retry,
        ),
        413 => info(
            "La petición de emparejamiento es demasiado grande.",
            None,
            FailureKind::Retry,
        ),
        429 => info(
            "Demasiados intentos seguidos; espera un momento.",
            None,
            FailureKind::Retry,
        ),
        200 => info(
            "El equipo respondió algo que no es un emparejamiento.",
            None,
            FailureKind::Retry,
        ),
        status => info(
            format!("El equipo rechazó el emparejamiento ({status})."),
            Some("Vuelve a intentarlo."),
            FailureKind::Retry,
        ),
    }
}

/// Explain the answer to `POST /v1/workspaces`. A `422` names the field that
/// failed validation, so the screen can point at the agent, the folder or the
/// label instead of saying "invalid request".
#[flutter_rust_bridge::frb(sync)]
pub fn explain_create_error(status: u16, body: String) -> ErrorInfo {
    if status == 422 {
        return match validation_field(&body).as_deref() {
            Some("agent") => info(
                "Ese agente no está en el catálogo del equipo.",
                Some("Elige otro agente de la lista."),
                FailureKind::Retry,
            ),
            Some("cwd") => info(
                "Esa carpeta no existe en el equipo.",
                Some("Escribe una ruta del host que exista y sea una carpeta."),
                FailureKind::Retry,
            ),
            Some("label") => info(
                "La etiqueta no es válida.",
                Some("Usa hasta 64 caracteres sin caracteres de control."),
                FailureKind::Retry,
            ),
            _ => info("La petición no es válida.", None, FailureKind::Retry),
        };
    }
    if status == 409 {
        if body.contains("pane_limit") {
            return info(
                "El host ya tiene seis paneles vivos.",
                Some("Detén un agente en Equipo y vuelve a intentarlo."),
                FailureKind::Retry,
            );
        }
        return info(
            "El equipo no pudo iniciar el agente.",
            Some("Revisa el catálogo y el comando del agente en el host."),
            FailureKind::Retry,
        );
    }
    explain_http_error(status, body, WriteAction::Create)
}

/// The result of a failed stop. `gone` means the pane was already removed, so
/// the detail should leave the list with a "ya había terminado" notice instead
/// of showing the dead-pane error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopFailure {
    pub gone: bool,
    pub error: ErrorInfo,
}

#[flutter_rust_bridge::frb(sync)]
pub fn classify_stop_error(status: u16, body: String) -> StopFailure {
    let (code, message) = parse_error_body(&body);
    let gone = status == 400 && code.as_deref() == Some("pane_gone");
    StopFailure {
        gone,
        error: if gone {
            info(
                "El agente ya había terminado.",
                Some("Actualiza la lista."),
                FailureKind::Retry,
            )
        } else {
            explain_error(status, code, message, WriteAction::Stop)
        },
    }
}

pub(crate) fn validation_field(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if let Some(field) = value
        .get("detail")
        .and_then(|detail| detail.get("field"))
        .and_then(|field| field.as_str())
    {
        return Some(field.to_string());
    }
    let errors = value
        .get("detail")
        .and_then(|detail| detail.get("errors"))
        .or_else(|| value.get("errors"))?
        .as_array()?;
    let loc = errors.first()?.get("loc")?.as_array()?;
    loc.first()?.as_str().map(str::to_string)
}

pub(crate) fn info(
    message: impl Into<String>,
    action: Option<&str>,
    kind: FailureKind,
) -> ErrorInfo {
    ErrorInfo {
        message: message.into(),
        action: action.map(str::to_string),
        kind,
    }
}

pub(crate) fn parse_error_body(body: &str) -> (Option<String>, String) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return (None, short(body));
    };
    let Some(object) = value.as_object() else {
        return (None, short(body));
    };

    match object.get("detail") {
        Some(serde_json::Value::String(detail)) => (None, detail.clone()),
        Some(serde_json::Value::Object(detail)) => (
            detail
                .get("code")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            detail
                .get("message")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    detail
                        .get("code")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| short(body)),
        ),
        _ => {
            let code = object
                .get("code")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let message = object
                .get("message")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| code.clone())
                .unwrap_or_else(|| short(body));
            (code, message)
        }
    }
}

fn short(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "no detail".to_string();
    }
    trimmed.chars().take(120).collect()
}

fn dead_panel() -> ErrorInfo {
    info(
        "Ese panel ya no existe.",
        Some("Actualiza la lista y elige otro agente."),
        FailureKind::Retry,
    )
}

/// A pane recovered from a previous host run: the row is still listed with its
/// last screen, but its process is gone, so driving it answers 409.
fn ended_panel() -> ErrorInfo {
    info(
        "El agente ya terminó; este panel es una instantánea de su última pantalla.",
        Some("Actualiza la lista; para seguir, inicia una conversación nueva."),
        FailureKind::Retry,
    )
}

fn looks_like_a_dead_panel(message: &str) -> bool {
    message.contains("not found") || message.contains("pane_gone")
}

fn forbidden(action: WriteAction) -> ErrorInfo {
    let (permission, purpose) = match action {
        WriteAction::Read => ("read_state", "leer el estado"),
        WriteAction::Respond => ("respond", "actuar sobre el agente"),
        WriteAction::Keys => ("send_key", "enviar teclas"),
        WriteAction::Seen => ("read_state", "marcar el agente como visto"),
        WriteAction::Create => ("respond", "crear una conversación"),
        WriteAction::Stop => ("respond", "detener el agente"),
        WriteAction::Admin => ("admin", "gestionar dispositivos"),
    };
    info(
        format!(
            "Falta el permiso {} para {purpose}.",
            permission.to_uppercase()
        ),
        Some(&format!(
            "Concédelo en el host: host devices grant <device_id> --permission {permission}"
        )),
        FailureKind::Permission,
    )
}

/// One action code becomes text and an action. Shared by the gateway's `400`
/// and by the local validation of a write plan, so there is one wording.
pub(crate) fn action_problem(code: &str) -> Option<ErrorInfo> {
    Some(match code {
        "pane_gone" => dead_panel(),
        "bad_pane" => info(
            "Ese panel no es válido.",
            Some("Actualiza la lista y elige otro agente."),
            FailureKind::Retry,
        ),
        "empty_prompt" | "empty_message" => info(
            "El mensaje está vacío.",
            Some("Escribe algo antes de enviar."),
            FailureKind::Retry,
        ),
        "message_too_long" => info(
            "El mensaje es demasiado largo.",
            Some("Acórtalo a 32 KiB y vuelve a intentarlo."),
            FailureKind::Retry,
        ),
        "bad_client_id" => info(
            "No se pudo preparar el envío.",
            Some("Vuelve a intentarlo."),
            FailureKind::Retry,
        ),
        "bad_conversation" => info(
            "Esa conversación no es válida.",
            Some("Actualiza el historial y elige otra."),
            FailureKind::Retry,
        ),
        "bad_delegation" => info(
            "Esa delegación no es válida.",
            Some("Actualiza el timeline y vuelve a intentarlo."),
            FailureKind::Retry,
        ),
        "bad_section" => info(
            "Esa sección no es válida.",
            Some("Actualiza el historial y vuelve a intentarlo."),
            FailureKind::Retry,
        ),
        "bad_title" => info(
            "El título no es válido.",
            Some("Usa hasta 200 caracteres sin caracteres de control."),
            FailureKind::Retry,
        ),
        "bad_section_name" => info(
            "El nombre de la sección no es válido.",
            Some("Usa hasta 64 caracteres sin caracteres de control."),
            FailureKind::Retry,
        ),
        "control_characters" => info(
            "El mensaje contiene caracteres de control.",
            Some("Quita los caracteres invisibles y vuelve a enviarlo."),
            FailureKind::Retry,
        ),
        "no_keys" | "empty_key" => {
            info("No hay ninguna tecla que enviar.", None, FailureKind::Retry)
        }
        "too_many_keys" => info(
            "Demasiadas teclas en una sola acción.",
            None,
            FailureKind::Retry,
        ),
        "confirmation_required" => info(
            "Esa tecla cambia trabajo en curso y necesita confirmación.",
            None,
            FailureKind::Retry,
        ),
        "key_not_allowed" => info("Esa tecla no está permitida.", None, FailureKind::Retry),
        "unsupported_type" | "type_mismatch" => info(
            "Ese tipo de imagen no está permitido.",
            None,
            FailureKind::Retry,
        ),
        "invalid_image" | "empty_image" => {
            info("La imagen no se pudo leer.", None, FailureKind::Retry)
        }
        "image_too_large" => info(
            "La imagen es demasiado grande.",
            Some("Usa una imagen de hasta 5 MiB."),
            FailureKind::Retry,
        ),
        "invalid_body" => info("La petición no es válida.", None, FailureKind::Retry),
        _ => return None,
    })
}

fn bad_request(code: Option<&str>, message: &str) -> ErrorInfo {
    if code == Some("bad_pane") && message.contains("no longer known") {
        return dead_panel();
    }
    code.and_then(action_problem).unwrap_or_else(|| {
        info(
            format!("El gateway rechazó la petición: {message}"),
            None,
            FailureKind::Retry,
        )
    })
}

#[cfg(test)]
mod tests;
