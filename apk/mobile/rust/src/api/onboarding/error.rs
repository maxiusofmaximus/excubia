//! What a refused onboarding call means on screen. The codes come from
//! `errors.md` («Hire and onboarding errors»); the text is written here, so the
//! UI never shows a raw code.

use crate::api::error::{
    explain_http_error, info, parse_error_body, validation_field, ErrorInfo, FailureKind,
    WriteAction,
};

/// A refused change to the team: rename, avatar, dismiss or hire. The hire codes
/// are read here; everything else goes to the onboarding explanation.
#[flutter_rust_bridge::frb(sync)]
pub fn explain_hire_error(status: u16, body: String) -> ErrorInfo {
    let (code, _) = parse_error_body(&body);
    match (status, code.as_deref()) {
        (404, Some("no_such_hire")) => info(
            "Ese agente ya no está en tu equipo.",
            Some("Actualiza la lista."),
            FailureKind::Retry,
        ),
        (409, Some("hire_not_active")) => info(
            "Ese agente ya fue despedido.",
            Some("Actualiza la lista."),
            FailureKind::Retry,
        ),
        (422, Some("invalid_name")) => info(
            "El nombre debe tener entre 1 y 40 caracteres, sin caracteres de control.",
            None,
            FailureKind::Retry,
        ),
        (422, Some("unknown_role")) => info(
            "Ese profesional ya no está en el catálogo.",
            Some("Actualiza la lista."),
            FailureKind::Retry,
        ),
        (422, Some("invalid_field")) if validation_field(&body).as_deref() == Some("avatar") => {
            info(
                "Ese personaje no está disponible.",
                None,
                FailureKind::Retry,
            )
        }
        _ => explain_onboarding_error(status, body),
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn explain_onboarding_error(status: u16, body: String) -> ErrorInfo {
    let (code, _) = parse_error_body(&body);
    match (status, code.as_deref()) {
        (403, _) => info(
            "Este dispositivo no tiene permiso para contratar agentes.",
            Some("Pide a quien administra el equipo que le conceda el permiso APPROVE."),
            FailureKind::Permission,
        ),
        (409, Some("area_full")) => info(
            "Esa área ya tiene 8 agentes.",
            Some("Quita alguno y vuelve a intentarlo."),
            FailureKind::Retry,
        ),
        (422, Some("unknown_role" | "unknown_area")) => info(
            "El catálogo de roles ha cambiado.",
            Some("Vuelve a abrir el asistente."),
            FailureKind::Retry,
        ),
        (422, Some("invalid_field")) => invalid_field(&body),
        _ => explain_http_error(status, body, WriteAction::Admin),
    }
}

fn invalid_field(body: &str) -> ErrorInfo {
    match validation_field(body).as_deref() {
        Some("roles") => info(
            "Uno de los roles no pertenece a las áreas elegidas.",
            Some("Quítalo de la lista y vuelve a intentarlo."),
            FailureKind::Retry,
        ),
        Some("areas") => info("Elige al menos un área.", None, FailureKind::Retry),
        _ => explain_http_error(422, body.to_string(), WriteAction::Admin),
    }
}
