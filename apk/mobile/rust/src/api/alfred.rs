//! `/v1/alfred/backend`: the engine of Alfred, Claude or the local model. The route and
//! the body are built here and a refused change is explained here; Dart only sends the bytes.

use serde_json::json;

use crate::api::chat::plan::{get, post, ChatPlan};
use crate::api::error::{explain_http_error, info, ErrorInfo, FailureKind, WriteAction};

const BACKEND: &str = "/v1/alfred/backend";

#[flutter_rust_bridge::frb(sync)]
pub fn plan_alfred_backend_read() -> ChatPlan {
    get(BACKEND.to_string())
}

/// Asks for the engine `local` or `claude`. Any other name is refused before the wire.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_alfred_backend_set(backend: String) -> ChatPlan {
    if !matches!(backend.as_str(), "local" | "claude") {
        return ChatPlan {
            request: None,
            problem: Some(info("El motor pedido no existe.", None, FailureKind::Retry)),
        };
    }
    post(
        BACKEND.to_string(),
        json!({ "backend": backend }).as_object().cloned().unwrap_or_default(),
    )
}

/// What a refused engine change says on screen. The 403, 404 and 409 answers have their
/// own words; anything else is explained like the rest of the host.
#[flutter_rust_bridge::frb(sync)]
pub fn explain_alfred_backend_error(status: u16, body: String) -> ErrorInfo {
    match status {
        403 => info(
            "Este móvil no puede cambiar el motor de Alfred.",
            None,
            FailureKind::Permission,
        ),
        404 => info(
            "Tu PC no admite este ajuste; actualiza Excubia en el PC.",
            None,
            FailureKind::Retry,
        ),
        409 => info(
            "Alfred ya está cambiando de modelo.",
            None,
            FailureKind::Retry,
        ),
        _ => explain_http_error(status, body, WriteAction::Admin),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_read_is_a_get_of_the_engine_state() {
        let call = plan_alfred_backend_read().request.expect("a request");
        assert_eq!((call.method.as_str(), call.target.as_str()), ("GET", BACKEND));
        assert!(call.body.is_empty());
    }

    #[test]
    fn a_change_posts_the_engine_name_and_refuses_any_other() {
        let call = plan_alfred_backend_set("local".to_string())
            .request
            .expect("a request");
        assert_eq!(call.method, "POST");
        assert_eq!(call.body, br#"{"backend":"local"}"#);

        let refused = plan_alfred_backend_set("gpt".to_string());
        assert!(refused.request.is_none());
        assert!(refused.problem.is_some());
    }

    #[test]
    fn the_refusals_the_screen_names_have_their_own_words() {
        let forbidden = explain_alfred_backend_error(403, String::new());
        assert_eq!(forbidden.kind, FailureKind::Permission);
        assert!(forbidden.message.contains("no puede cambiar el motor"));

        let old_host = explain_alfred_backend_error(404, String::new());
        assert!(old_host.message.contains("actualiza Excubia en el PC"));

        let busy = explain_alfred_backend_error(409, String::new());
        assert!(busy.message.contains("cambiando de modelo"));
    }
}
