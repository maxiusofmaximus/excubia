use super::{ErrorInfo, WriteAction, GRANT_HINT};
use crate::ui_lang::UiLang;

pub(super) fn forbidden_info(action: WriteAction, lang: UiLang) -> ErrorInfo {
    let message = match action {
        WriteAction::Respond | WriteAction::CreateWorkspace | WriteAction::Stop => lang.pick(
            "Este equipo solo puede leer; falta el permiso RESPOND.",
            "This device can only read; the RESPOND permission is missing.",
        ),
        WriteAction::Keys => lang.pick(
            "Este equipo solo puede leer; falta el permiso SEND_KEY.",
            "This device can only read; the SEND_KEY permission is missing.",
        ),
        WriteAction::SetDefault => lang.pick(
            "Este equipo no tiene el permiso APPROVE.",
            "This device does not have the APPROVE permission.",
        ),
        WriteAction::Seen | WriteAction::Read => lang.pick(
            "Este equipo no tiene el permiso READ_STATE.",
            "This device does not have the READ_STATE permission.",
        ),
    };
    let grant = lang.pick("Concédelo en el host", "Grant it on the host");
    ErrorInfo {
        message: message.to_string(),
        action: Some(format!("{grant}: {GRANT_HINT}")),
    }
}

pub(super) fn invalid_field_info(field: Option<&str>, lang: UiLang) -> ErrorInfo {
    let (message, action) = match field {
        Some("agent") => (
            lang.pick(
                "Ese agente no está en el catálogo del host.",
                "That agent is not in the host's catalog.",
            ),
            Some(lang.pick("Elige uno de la lista.", "Choose one from the list.")),
        ),
        Some("cwd") => (
            lang.pick(
                "La carpeta no existe o no es un directorio.",
                "The folder does not exist or is not a directory.",
            ),
            Some(lang.pick("Elige otra carpeta.", "Choose another folder.")),
        ),
        Some("label") => (
            lang.pick(
                "La etiqueta no es válida (máximo 64 caracteres, sin caracteres de control).",
                "The label is not valid (64 characters at most, no control characters).",
            ),
            Some(lang.pick(
                "Cámbiala y vuelve a intentarlo.",
                "Change it and try again.",
            )),
        ),
        _ => (
            lang.pick("La petición no es válida.", "The request is not valid."),
            Some(lang.pick(
                "Revisa los datos y vuelve a intentarlo.",
                "Check the details and try again.",
            )),
        ),
    };
    ErrorInfo {
        message: message.to_string(),
        action: action.map(str::to_string),
    }
}

pub(super) fn create_refused_info(message: &str, lang: UiLang) -> ErrorInfo {
    let text = if lang.is_en() {
        format!("The host could not start the agent: {message}")
    } else {
        format!("El host no pudo iniciar el agente: {message}")
    };
    ErrorInfo {
        message: text,
        action: Some(
            lang.pick(
                "Revisa el comando del agente en agents.toml.",
                "Check the agent command in agents.toml.",
            )
            .to_string(),
        ),
    }
}

pub(super) fn stop_refused_info(message: &str, lang: UiLang) -> ErrorInfo {
    let text = if lang.is_en() {
        format!("This agent cannot be stopped: {message}")
    } else {
        format!("No se puede detener este agente: {message}")
    };
    ErrorInfo {
        message: text,
        action: Some(
            lang.pick(
                "Inicia una nueva conversación.",
                "Start a new conversation.",
            )
            .to_string(),
        ),
    }
}

fn dead_panel_info(lang: UiLang) -> ErrorInfo {
    ErrorInfo {
        message: lang
            .pick("Ese panel ya no existe.", "That pane no longer exists.")
            .to_string(),
        action: Some(
            lang.pick(
                "Actualiza la lista y elige otro agente.",
                "Refresh the list and choose another agent.",
            )
            .to_string(),
        ),
    }
}

pub(super) fn refusal_info(message: &str, lang: UiLang) -> ErrorInfo {
    if message.contains("not found") || message.contains("pane_gone") {
        dead_panel_info(lang)
    } else {
        let text = if lang.is_en() {
            format!("The agent did not accept the action: {message}")
        } else {
            format!("El agente no aceptó la acción: {message}")
        };
        ErrorInfo {
            message: text,
            action: Some(
                lang.pick(
                    "Actualiza el estado y vuelve a intentarlo.",
                    "Refresh the status and try again.",
                )
                .to_string(),
            ),
        }
    }
}

pub(super) fn bad_request_info(code: Option<&str>, message: &str, lang: UiLang) -> ErrorInfo {
    let (text, action) = match code {
        Some("empty_prompt") => (
            lang.pick("El mensaje está vacío.", "The message is empty."),
            Some(lang.pick(
                "Escribe algo antes de enviar.",
                "Write something before sending.",
            )),
        ),
        Some("control_characters") => (
            lang.pick(
                "El mensaje contiene caracteres de control.",
                "The message contains control characters.",
            ),
            Some(lang.pick(
                "Quita los caracteres invisibles y vuelve a enviarlo.",
                "Remove the invisible characters and send it again.",
            )),
        ),
        Some("empty_key" | "no_keys") => (
            lang.pick(
                "No hay ninguna tecla que enviar.",
                "There is no key to send.",
            ),
            None,
        ),
        Some("too_many_keys") => (
            lang.pick(
                "Demasiadas teclas en una sola acción.",
                "Too many keys in one action.",
            ),
            None,
        ),
        Some("confirmation_required") => (
            lang.pick(
                "Esa tecla cambia trabajo en curso y necesita confirmación.",
                "That key changes work in progress and needs confirmation.",
            ),
            None,
        ),
        Some("key_not_allowed") => (
            lang.pick("Esa tecla no está permitida.", "That key is not allowed."),
            None,
        ),
        Some("pane_gone") => return dead_panel_info(lang),
        Some("bad_pane") if message.contains("no longer known") => {
            return dead_panel_info(lang)
        }
        Some("bad_pane") => (
            lang.pick("Ese panel no es válido.", "That pane is not valid."),
            Some(lang.pick(
                "Actualiza la lista y elige otro agente.",
                "Refresh the list and choose another agent.",
            )),
        ),
        Some("invalid_body") => (
            lang.pick("La petición no es válida.", "The request is not valid."),
            None,
        ),
        _ => {
            let text = if lang.is_en() {
                format!("The PC rejected the request: {message}")
            } else {
                format!("El PC rechazó la petición: {message}")
            };
            return ErrorInfo {
                message: text,
                action: None,
            };
        }
    };
    ErrorInfo {
        message: text.to_string(),
        action: action.map(str::to_string),
    }
}
