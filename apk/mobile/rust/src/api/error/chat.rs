//! One place where a chat HTTP outcome becomes Spanish text, an action and a
//! stable code. The screen branches on the code, never on a raw status.

use super::{
    explain_error, parse_error_body, validation_field, ErrorInfo, FailureKind, WriteAction,
};

/// A refused chat call. `code` is the contract detail when the host sent one;
/// `offline` and `forbidden` are the two local names for statuses without a
/// chat code. `retry_after` is the `Retry-After` header in seconds, when the
/// host sent one (budget `429`, and `agent_busy` in some builds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatFailure {
    pub code: String,
    pub message: String,
    pub action: Option<String>,
    pub kind: FailureKind,
    pub retry_after: Option<u32>,
    /// The timeline shows this failure as a card. Other codes only reach the
    /// status line.
    pub shown_in_timeline: bool,
    /// The host kept the user's turn, so the optimistic bubble is dropped and
    /// the timeline is read again instead of resending.
    pub kept_by_host: bool,
}

/// Codes the timeline shows as a card. This list is the only copy in the app.
const SHOWN_IN_TIMELINE: &[&str] = &[
    "offline",
    "seat_unavailable",
    "agent_busy",
    "rate_limited",
    "forbidden",
    "conversation_archived",
    "no_such_conversation",
    "depth_exceeded",
    "pane_limit",
    "cycle_detected",
    "turn_budget",
    "hour_budget",
    "no_such_delegation",
    "delegation_cancelled",
];

const KEPT_BY_HOST: &[&str] = &["seat_unavailable", "agent_busy", "rate_limited"];

impl ChatFailure {
    fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        action: Option<String>,
        kind: FailureKind,
        retry_after: Option<u32>,
    ) -> Self {
        let code = code.into();
        Self {
            shown_in_timeline: SHOWN_IN_TIMELINE.contains(&code.as_str()),
            kept_by_host: KEPT_BY_HOST.contains(&code.as_str()),
            code,
            message: message.into(),
            action,
            kind,
            retry_after,
        }
    }
}

/// The card for a conversation that cannot reach the host. The text comes
/// from the screen (it names the agent and the last sync); the code and its
/// classification come from here.
#[flutter_rust_bridge::frb(sync)]
pub fn offline_chat_failure(message: String, action: String) -> ChatFailure {
    ChatFailure::new("offline", message, Some(action), FailureKind::Retry, None)
}

fn with_action(
    code: &str,
    message: &str,
    action: String,
    kind: FailureKind,
    retry_after: Option<u32>,
) -> ChatFailure {
    ChatFailure::new(code, message, Some(action), kind, retry_after)
}

fn says(code: &str, message: &str, wanted: &str) -> bool {
    code == wanted || message.contains(wanted)
}

#[flutter_rust_bridge::frb(sync)]
pub fn explain_chat_error(
    status: u16,
    body: String,
    retry_after: Option<u32>,
    action: WriteAction,
) -> ChatFailure {
    let (code, message) = parse_error_body(&body);
    let code = code.unwrap_or_default();
    match status {
        0 => ChatFailure::new(
            "offline",
            "Sin conexión con el equipo.",
            Some("Lo que ya está cargado sigue legible; reconecta para enviar.".to_string()),
            FailureKind::Retry,
            None,
        ),
        403 => {
            let info = explain_error(status, Some(code), message, action);
            ChatFailure::new(
                "forbidden",
                info.message,
                info.action,
                FailureKind::Permission,
                None,
            )
        }
        404 if says(&code, &message, "no_such_conversation") => ChatFailure::new(
            "no_such_conversation",
            "La conversación ya no existe en el equipo.",
            Some("Actualiza el historial y elige otra.".to_string()),
            FailureKind::Retry,
            None,
        ),
        404 if says(&code, &message, "no_such_delegation") => ChatFailure::new(
            "no_such_delegation",
            "La delegación ya no existe en el equipo.",
            Some("Actualiza el timeline y vuelve a intentarlo.".to_string()),
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "conversation_busy") => ChatFailure::new(
            "conversation_busy",
            "Esta conversación tiene algo en marcha (un turno, una tarea o una aprobación). \
             Páralo o resuélvelo antes de borrarla.",
            None,
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "delegation_cancelled") => ChatFailure::new(
            "delegation_cancelled",
            "La delegación ya terminó o fue cancelada.",
            Some("Actualiza la tarjeta; el historial se conserva.".to_string()),
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "depth_exceeded") => ChatFailure::new(
            "depth_exceeded",
            "La delegación supera la profundidad permitida.",
            Some(
                "Alfred delega en el lead y el lead en un especialista; no hay un tercer nivel."
                    .to_string(),
            ),
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "pane_limit") => ChatFailure::new(
            "pane_limit",
            "El host ya tiene seis paneles vivos.",
            Some("Detén un agente en Equipo y vuelve a intentarlo.".to_string()),
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "cycle_detected") => ChatFailure::new(
            "cycle_detected",
            "Esa delegación repetiría un ciclo.",
            Some("Cambia el destinatario o la tarea y vuelve a intentarlo.".to_string()),
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "turn_budget") => ChatFailure::new(
            "turn_budget",
            "Este turno ya hizo doce delegaciones.",
            Some("Espera al siguiente turno para repartir más trabajo.".to_string()),
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "hour_budget") => ChatFailure::new(
            "hour_budget",
            "Se alcanzaron sesenta delegaciones en la última hora.",
            Some("Espera antes de delegar más trabajo.".to_string()),
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "scope_denied") => ChatFailure::new(
            "scope_denied",
            "Ese asiento no puede hacer esta delegación.",
            Some("Un especialista no delega; el trabajo lo reparte su lead.".to_string()),
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "seat_limit") => ChatFailure::new(
            "seat_limit",
            "El proyecto ya tiene ocho asientos.",
            Some("Reutiliza un asiento o crea otro proyecto.".to_string()),
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "lead_exists") => ChatFailure::new(
            "lead_exists",
            "El proyecto ya tiene un lead.",
            Some("Un proyecto tiene un único lead en este MVP.".to_string()),
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "project_exists") => ChatFailure::new(
            "project_exists",
            "Ya hay un proyecto en esa carpeta.",
            Some("Usa el proyecto existente en Equipo.".to_string()),
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "seat_unavailable") => ChatFailure::new(
            "seat_unavailable",
            "Alfred no pudo arrancar: el asiento no tiene un CLI disponible.",
            Some("Revisa el motor y el CLI en el host; el historial se conserva.".to_string()),
            FailureKind::Retry,
            None,
        ),
        409 if says(&code, &message, "agent_busy") => with_action(
            "agent_busy",
            "La cola de Alfred está llena.",
            match retry_after {
                Some(seconds) => format!("Reintenta en {seconds} s; el mensaje no se envió."),
                None => {
                    "Reintenta cuando termine el turno actual; el mensaje no se envió.".to_string()
                }
            },
            FailureKind::Retry,
            retry_after,
        ),
        409 if says(&code, &message, "conversation_archived") => ChatFailure::new(
            "conversation_archived",
            "Esta conversación está archivada.",
            Some("Desarchívala en el historial para volver a escribir.".to_string()),
            FailureKind::Retry,
            None,
        ),
        422 if says(&code, &message, "unknown_reply_to") => ChatFailure::new(
            "unknown_reply_to",
            "Ese mensaje ya no está en la conversación.",
            Some("Se quitó la cita; tu texto sigue escrito.".to_string()),
            FailureKind::Retry,
            None,
        ),
        422 => {
            let field = validation_field(&body);
            let (text, hint) = match field.as_deref() {
                Some("project_id") => (
                    "El proyecto de esa conversación no existe.",
                    "Crea la conversación desde el historial.",
                ),
                Some("section_id") => (
                    "Esa sección no existe.",
                    "Actualiza el historial y elige otra sección.",
                ),
                _ => (
                    "La petición no es válida.",
                    "Revisa el texto y vuelve a intentarlo.",
                ),
            };
            ChatFailure::new(
                "invalid_body",
                text,
                Some(hint.to_string()),
                FailureKind::Retry,
                None,
            )
        }
        429 => with_action(
            "rate_limited",
            "El equipo está limitando las peticiones.",
            match retry_after {
                Some(seconds) => format!("Espera {seconds} s y vuelve a intentarlo."),
                None => "Espera unos segundos y vuelve a intentarlo.".to_string(),
            },
            FailureKind::Retry,
            retry_after,
        ),
        _ => {
            let info = explain_error(status, (!code.is_empty()).then_some(code), message, action);
            ChatFailure::new("http", info.message, info.action, info.kind, retry_after)
        }
    }
}

/// A refused stop has its own wording. A conversation the host does not know
/// keeps the chat's wording; a 404 or 405 without that code means an old host
/// that has no stop route.
#[flutter_rust_bridge::frb(sync)]
pub fn explain_chat_stop_error(status: u16, body: String, retry_after: Option<u32>) -> ChatFailure {
    let (code, message) = parse_error_body(&body);
    let code = code.unwrap_or_default();
    match status {
        403 => {
            let grant = explain_chat_error(status, body, retry_after, WriteAction::Respond);
            ChatFailure::new(
                "forbidden",
                "No tienes permiso para detener esto desde este dispositivo.",
                grant.action,
                FailureKind::Permission,
                None,
            )
        }
        404 if says(&code, &message, "no_such_conversation") => {
            explain_chat_error(status, body, retry_after, WriteAction::Respond)
        }
        404 | 405 => ChatFailure::new(
            "unsupported_host",
            "Este equipo no admite detener todavía.",
            Some("Actualiza el equipo para poder detener el turno.".to_string()),
            FailureKind::Retry,
            None,
        ),
        _ => ChatFailure::new(
            "stop_failed",
            "No se pudo detener. Inténtalo de nuevo.",
            None,
            FailureKind::Retry,
            None,
        ),
    }
}

/// A host without the counts route answers 404 or 405: the new chips stay hidden.
#[flutter_rust_bridge::frb(sync)]
pub fn chat_counts_unsupported(status: u16) -> bool {
    matches!(status, 404 | 405)
}

/// Map a locally refused plan (no host call was made) to the same failure
/// shape the screen renders for host refusals.
#[flutter_rust_bridge::frb(sync)]
pub fn local_chat_failure(error: ErrorInfo) -> ChatFailure {
    ChatFailure::new("local", error.message, error.action, error.kind, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explain(status: u16, body: &str) -> ChatFailure {
        explain_chat_error(status, body.to_string(), None, WriteAction::Read)
    }

    #[test]
    fn a_missing_seat_and_a_full_queue_are_distinct() {
        let seat = explain(409, r#"{"detail":"seat_unavailable"}"#);
        assert_eq!(seat.code, "seat_unavailable");
        assert!(seat.message.contains("CLI"));

        let busy = explain(409, r#"{"detail":"agent_busy"}"#);
        assert_eq!(busy.code, "agent_busy");
        assert!(busy.message.contains("cola"));
    }

    #[test]
    fn retry_after_is_carried_into_the_action() {
        let failure = explain_chat_error(
            429,
            r#"{"detail":{"code":"budget","message":"slow down"}}"#.to_string(),
            Some(18),
            WriteAction::Respond,
        );
        assert_eq!(failure.retry_after, Some(18));
        assert_eq!(
            failure.action.as_deref(),
            Some("Espera 18 s y vuelve a intentarlo.")
        );
        assert_eq!(failure.code, "rate_limited");
    }

    #[test]
    fn seat_and_queue_errors_keep_the_busy_code() {
        let busy = explain(409, r#"{"code":"agent_busy","message":"full"}"#);
        assert_eq!(busy.code, "agent_busy");
        assert!(busy.action.unwrap().contains("turno"));
    }

    #[test]
    fn an_archived_conversation_says_how_to_fix_it() {
        let archived = explain(409, r#"{"detail":"conversation_archived"}"#);
        assert_eq!(archived.code, "conversation_archived");
        assert!(archived.action.unwrap().contains("Desarchívala"));
    }

    #[test]
    fn a_403_uses_the_canonical_grant_and_the_action_permission() {
        let read = explain_chat_error(
            403,
            r#"{"detail":"forbidden"}"#.to_string(),
            None,
            WriteAction::Read,
        );
        assert_eq!(read.kind, FailureKind::Permission);
        assert!(read.message.contains("READ_STATE"));
        assert!(read.action.unwrap().contains("--permission read_state"));

        let send = explain_chat_error(
            403,
            r#"{"detail":"forbidden"}"#.to_string(),
            None,
            WriteAction::Respond,
        );
        assert!(send.message.contains("RESPOND"));
        assert!(send.action.unwrap().contains("--permission respond"));
    }

    #[test]
    fn unknown_conversation_and_validation_fields_are_named() {
        let missing = explain(404, r#"{"detail":"no_such_conversation"}"#);
        assert_eq!(missing.code, "no_such_conversation");

        let section = explain(
            422,
            r#"{"detail":{"code":"unknown_section","field":"section_id","message":"x"}}"#,
        );
        assert!(section.message.contains("sección"));

        let quote = explain(
            422,
            r#"{"detail":{"code":"unknown_reply_to","field":"reply_to","message":"x"}}"#,
        );
        assert_eq!(quote.code, "unknown_reply_to");
        assert!(!quote.shown_in_timeline && !quote.kept_by_host);
    }

    #[test]
    fn a_busy_conversation_says_what_to_stop_before_deleting() {
        let busy = explain(409, r#"{"detail":"conversation_busy"}"#);
        assert_eq!(busy.code, "conversation_busy");
        assert!(busy.message.ends_with("antes de borrarla."));
        assert!(busy.action.is_none());
        assert!(!busy.shown_in_timeline && !busy.kept_by_host);
    }

    #[test]
    fn every_delegation_refusal_has_its_own_clear_message() {
        for (code, needle) in [
            ("depth_exceeded", "profundidad"),
            ("pane_limit", "seis paneles"),
            ("cycle_detected", "ciclo"),
            ("turn_budget", "doce"),
            ("hour_budget", "sesenta"),
            ("scope_denied", "asiento"),
            ("seat_limit", "ocho asientos"),
        ] {
            let failure = explain(409, &format!(r#"{{"detail":"{code}"}}"#));
            assert_eq!(failure.code, code);
            assert!(
                failure.message.contains(needle),
                "{code} must explain the limit: {}",
                failure.message
            );
            assert!(failure.action.is_some(), "{code} must offer a recovery");
        }
    }

    #[test]
    fn a_finished_delegation_and_a_missing_one_are_distinct() {
        let cancelled = explain(409, r#"{"detail":"delegation_cancelled"}"#);
        assert_eq!(cancelled.code, "delegation_cancelled");
        assert!(cancelled.action.unwrap().contains("Actualiza"));

        let missing = explain(404, r#"{"detail":"no_such_delegation"}"#);
        assert_eq!(missing.code, "no_such_delegation");
        assert!(missing.action.unwrap().contains("timeline"));
    }

    #[test]
    fn a_refused_stop_says_why_and_an_old_host_is_named() {
        let stop = |status, body: &str| explain_chat_stop_error(status, body.to_string(), None);

        let denied = stop(403, r#"{"detail":"forbidden"}"#);
        assert_eq!(denied.code, "forbidden");
        assert_eq!(denied.kind, FailureKind::Permission);
        assert!(denied.message.contains("detener"));
        assert!(denied.action.unwrap().contains("--permission respond"));

        let missing = stop(404, r#"{"detail":"no_such_conversation"}"#);
        assert_eq!(missing.code, "no_such_conversation");

        for status in [404, 405] {
            let old = stop(status, r#"{"detail":"Not Found"}"#);
            assert_eq!(old.code, "unsupported_host");
        }

        let red = stop(0, "");
        assert_eq!(red.code, "stop_failed");
        assert_eq!(red.message, "No se pudo detener. Inténtalo de nuevo.");
    }

    #[test]
    fn an_old_host_without_the_counts_route_is_recognised() {
        assert!(chat_counts_unsupported(404));
        assert!(chat_counts_unsupported(405));
        assert!(!chat_counts_unsupported(403));
        assert!(!chat_counts_unsupported(500));
    }

    #[test]
    fn transport_failures_read_as_offline() {
        let offline = explain(0, "");
        assert_eq!(offline.code, "offline");
        assert_eq!(offline.kind, FailureKind::Retry);
    }

    #[test]
    fn the_timeline_and_the_host_keep_rules_are_decided_here() {
        // A busy seat keeps the turn on the host: a card, and no resend.
        let busy = explain(409, r#"{"detail":"seat_unavailable"}"#);
        assert!(busy.shown_in_timeline && busy.kept_by_host);
        // A limit is a card but the host did not keep the turn.
        let limit = explain(409, r#"{"detail":"pane_limit"}"#);
        assert!(limit.shown_in_timeline && !limit.kept_by_host);
        // A code outside both lists only reaches the status line.
        let quiet = ChatFailure::new("unknown_code", "m", None, FailureKind::Retry, None);
        assert!(!quiet.shown_in_timeline && !quiet.kept_by_host);
        let offline = offline_chat_failure("sin conexión".into(), "reconecta".into());
        assert!(offline.shown_in_timeline && !offline.kept_by_host);
    }
}
