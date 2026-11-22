use super::{ErrorInfo, WriteAction, GRANT_HINT};

pub(super) fn forbidden_info(action: WriteAction) -> ErrorInfo {
    let message = match action {
        WriteAction::Respond | WriteAction::CreateWorkspace | WriteAction::Stop => {
            "Este equipo solo puede leer; falta el permiso RESPOND."
        }
        WriteAction::Keys => "Este equipo solo puede leer; falta el permiso SEND_KEY.",
        WriteAction::SetDefault => "Este equipo no tiene el permiso APPROVE.",
        WriteAction::Seen | WriteAction::Read => "Este equipo no tiene el permiso READ_STATE.",
    };
    ErrorInfo {
        message: message.to_string(),
        action: Some(format!("Concédelo en el host: {GRANT_HINT}")),
    }
}

pub(super) fn invalid_field_info(field: Option<&str>) -> ErrorInfo {
    let (message, action) = match field {
        Some("agent") => (
            "Ese agente no está en el catálogo del host.",
            Some("Elige uno de la lista."),
        ),
        Some("cwd") => (
            "La carpeta no existe o no es un directorio.",
            Some("Elige otra carpeta."),
        ),
        Some("label") => (
            "La etiqueta no es válida (máximo 64 caracteres, sin caracteres de control).",
            Some("Cámbiala y vuelve a intentarlo."),
        ),
        _ => (
            "La petición no es válida.",
            Some("Revisa los datos y vuelve a intentarlo."),
        ),
    };
    ErrorInfo {
        message: message.to_string(),
        action: action.map(str::to_string),
    }
}

pub(super) fn create_refused_info(message: &str) -> ErrorInfo {
    ErrorInfo {
        message: format!("El host no pudo iniciar el agente: {message}"),
        action: Some("Revisa el comando del agente en agents.toml.".to_string()),
    }
}

pub(super) fn stop_refused_info(message: &str) -> ErrorInfo {
    ErrorInfo {
        message: format!("No se puede detener este agente: {message}"),
        action: Some("Inicia una nueva conversación.".to_string()),
    }
}

fn dead_panel_info() -> ErrorInfo {
    ErrorInfo {
        message: "Ese panel ya no existe.".to_string(),
        action: Some("Actualiza la lista y elige otro agente.".to_string()),
    }
}

pub(super) fn refusal_info(message: &str) -> ErrorInfo {
    if message.contains("not found") || message.contains("pane_gone") {
        dead_panel_info()
    } else {
        ErrorInfo {
            message: format!("El agente no aceptó la acción: {message}"),
            action: Some("Actualiza el estado y vuelve a intentarlo.".to_string()),
        }
    }
}

pub(super) fn bad_request_info(code: Option<&str>, message: &str) -> ErrorInfo {
    let (text, action) = match code {
        Some("empty_prompt") => (
            "El mensaje está vacío.",
            Some("Escribe algo antes de enviar."),
        ),
        Some("control_characters") => (
            "El mensaje contiene caracteres de control.",
            Some("Quita los caracteres invisibles y vuelve a enviarlo."),
        ),
        Some("empty_key" | "no_keys") => ("No hay ninguna tecla que enviar.", None),
        Some("too_many_keys") => ("Demasiadas teclas en una sola acción.", None),
        Some("confirmation_required") => (
            "Esa tecla cambia trabajo en curso y necesita confirmación.",
            None,
        ),
        Some("key_not_allowed") => ("Esa tecla no está permitida.", None),
        Some("pane_gone") => return dead_panel_info(),
        Some("bad_pane") if message.contains("no longer known") => return dead_panel_info(),
        Some("bad_pane") => (
            "Ese panel no es válido.",
            Some("Actualiza la lista y elige otro agente."),
        ),
        Some("invalid_body") => ("La petición no es válida.", None),
        _ => {
            return ErrorInfo {
                message: format!("El PC rechazó la petición: {message}"),
                action: None,
            }
        }
    };
    ErrorInfo {
        message: text.to_string(),
        action: action.map(str::to_string),
    }
}
