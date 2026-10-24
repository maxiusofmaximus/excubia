//! Write actions as a request plan: route, body and validation in one place.
//!
//! Dart only executes the plan and renders the error the Rust core returns;
//! it never builds a pane URL or a JSON body itself.

use super::error::{action_problem, ErrorInfo, FailureKind};
use super::validate::{self, TextProblem};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentActionKind {
    Respond,
    Keys,
    Seen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAction {
    pub kind: AgentActionKind,
    pub pane_id: String,
    pub text: Option<String>,
    pub keys: Vec<String>,
    pub confirm: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpCall {
    pub method: String,
    pub target: String,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WritePlan {
    pub request: Option<HttpCall>,
    pub problem: Option<ErrorInfo>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_write(action: AgentAction) -> WritePlan {
    if !valid_pane_id(&action.pane_id) {
        return problem("bad_pane");
    }
    let route = match action.kind {
        AgentActionKind::Respond => "respond",
        AgentActionKind::Keys => "keys",
        AgentActionKind::Seen => "seen",
    };
    let target = format!("/v1/agents/{}/{route}", action.pane_id);
    match action.kind {
        AgentActionKind::Respond => respond_plan(action, target),
        AgentActionKind::Keys => keys_plan(action, target),
        AgentActionKind::Seen => plan(HttpCall {
            method: "POST".to_string(),
            target,
            body: Vec::new(),
        }),
    }
}

fn respond_plan(action: AgentAction, target: String) -> WritePlan {
    let text = action.text.unwrap_or_default();
    let text = match validate::message_text(&text) {
        Ok(text) => text,
        Err(TextProblem::Empty) => return problem("empty_prompt"),
        Err(TextProblem::ControlCharacters) => return problem("control_characters"),
        Err(TextProblem::TooLong) => return problem("message_too_long"),
    };
    match serde_json::to_vec(&serde_json::json!({ "text": text })) {
        Ok(body) => plan(HttpCall {
            method: "POST".to_string(),
            target,
            body,
        }),
        Err(_) => problem("invalid_body"),
    }
}

fn keys_plan(action: AgentAction, target: String) -> WritePlan {
    let keys: Vec<String> = action.keys.iter().map(|key| normalise_key(key)).collect();
    if keys.is_empty() {
        return problem("no_keys");
    }
    if keys.len() > 8 {
        return problem("too_many_keys");
    }
    if keys.iter().any(String::is_empty) {
        return problem("empty_key");
    }
    if keys.iter().any(|key| !allowed_key(key)) {
        return problem("key_not_allowed");
    }
    if keys.iter().any(|key| key == "ctrl+c") && !action.confirm {
        return problem("confirmation_required");
    }
    match serde_json::to_vec(&serde_json::json!({
        "keys": keys,
        "confirm": action.confirm,
    })) {
        Ok(body) => plan(HttpCall {
            method: "POST".to_string(),
            target,
            body,
        }),
        Err(_) => problem("invalid_body"),
    }
}

fn plan(request: HttpCall) -> WritePlan {
    WritePlan {
        request: Some(request),
        problem: None,
    }
}

fn problem(code: &str) -> WritePlan {
    let info = action_problem(code).unwrap_or_else(|| ErrorInfo {
        message: "La petición no es válida.".to_string(),
        action: None,
        kind: FailureKind::Retry,
    });
    WritePlan {
        request: None,
        problem: Some(info),
    }
}

fn normalise_key(key: &str) -> String {
    let key = key.trim().to_ascii_lowercase();
    if key == "escape" {
        "esc".to_string()
    } else {
        key
    }
}

fn allowed_key(key: &str) -> bool {
    matches!(
        key,
        "up" | "down"
            | "left"
            | "right"
            | "enter"
            | "esc"
            | "tab"
            | "space"
            | "backspace"
            | "home"
            | "end"
            | "pgup"
            | "pgdown"
            | "y"
            | "n"
            | "0"
            | "1"
            | "2"
            | "3"
            | "4"
            | "5"
            | "6"
            | "7"
            | "8"
            | "9"
            | "ctrl+c"
    )
}

fn valid_pane_id(pane_id: &str) -> bool {
    let Some((workspace, pane)) = pane_id.split_once(':') else {
        return false;
    };
    let Some(rest) = pane.strip_prefix('p') else {
        return false;
    };
    (1..=32).contains(&workspace.len())
        && workspace.chars().all(|c| c.is_ascii_alphanumeric())
        && (1..=32).contains(&rest.len())
        && rest.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Stop a live agent: `POST /v1/agents/{pane_id}/stop` with no body.
#[flutter_rust_bridge::frb(sync)]
pub fn plan_stop(pane_id: String) -> WritePlan {
    if !valid_pane_id(&pane_id) {
        return problem("bad_pane");
    }
    plan(HttpCall {
        method: "POST".to_string(),
        target: format!("/v1/agents/{pane_id}/stop"),
        body: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn respond(pane: &str, text: &str) -> AgentAction {
        AgentAction {
            kind: AgentActionKind::Respond,
            pane_id: pane.to_string(),
            text: Some(text.to_string()),
            keys: Vec::new(),
            confirm: false,
        }
    }

    fn keys(pane: &str, keys: &[&str], confirm: bool) -> AgentAction {
        AgentAction {
            kind: AgentActionKind::Keys,
            pane_id: pane.to_string(),
            text: None,
            keys: keys.iter().map(|key| (*key).to_string()).collect(),
            confirm,
        }
    }

    #[test]
    fn a_prompt_becomes_the_contract_call() {
        let plan = plan_write(respond("w1:p1", "y"));
        assert!(plan.problem.is_none());
        let call = plan.request.unwrap();
        assert_eq!(call.method, "POST");
        assert_eq!(call.target, "/v1/agents/w1:p1/respond");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&call.body).unwrap(),
            serde_json::json!({ "text": "y" })
        );
    }

    #[test]
    fn a_blank_or_control_character_prompt_is_refused_locally() {
        let blank = plan_write(respond("w1:p1", "   "));
        assert_eq!(blank.problem.unwrap().message, "El mensaje está vacío.");

        let control = plan_write(respond("w1:p1", "hola\u{7}"));
        assert_eq!(
            control.problem.unwrap().message,
            "El mensaje contiene caracteres de control."
        );

        let newline = plan_write(respond("w1:p1", "linea\nsiguiente\tok"));
        assert!(newline.problem.is_none());
    }

    #[test]
    fn keys_are_lower_cased_trimmed_and_escape_is_normalised() {
        let plan = plan_write(keys("w1:p1", &[" Escape ", "Y"], false));
        let call = plan.request.unwrap();
        assert_eq!(call.target, "/v1/agents/w1:p1/keys");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&call.body).unwrap(),
            serde_json::json!({ "keys": ["esc", "y"], "confirm": false })
        );
    }

    #[test]
    fn an_empty_key_list_is_refused() {
        assert_eq!(
            plan_write(keys("w1:p1", &[], false))
                .problem
                .unwrap()
                .message,
            "No hay ninguna tecla que enviar."
        );
        assert_eq!(
            plan_write(keys("w1:p1", &["  "], false))
                .problem
                .unwrap()
                .message,
            "No hay ninguna tecla que enviar."
        );
    }

    #[test]
    fn a_key_outside_the_allowlist_is_refused() {
        assert_eq!(
            plan_write(keys("w1:p1", &["rm -rf"], false))
                .problem
                .unwrap()
                .message,
            "Esa tecla no está permitida."
        );
    }

    #[test]
    fn ctrl_c_needs_confirmation_and_the_flag_travels() {
        assert_eq!(
            plan_write(keys("w1:p1", &["ctrl+c"], false))
                .problem
                .unwrap()
                .message,
            "Esa tecla cambia trabajo en curso y necesita confirmación."
        );
        let plan = plan_write(keys("w1:p1", &["ctrl+c"], true));
        let call = plan.request.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&call.body).unwrap(),
            serde_json::json!({ "keys": ["ctrl+c"], "confirm": true })
        );
    }

    #[test]
    fn too_many_keys_and_blank_keys_are_refused() {
        assert_eq!(
            plan_write(keys("w1:p1", &["y"; 9], false))
                .problem
                .unwrap()
                .message,
            "Demasiadas teclas en una sola acción."
        );
        assert_eq!(
            plan_write(keys("w1:p1", &[""], false))
                .problem
                .unwrap()
                .message,
            "No hay ninguna tecla que enviar."
        );
    }

    #[test]
    fn seen_is_a_post_with_no_body() {
        let action = AgentAction {
            kind: AgentActionKind::Seen,
            pane_id: "w1:p9".to_string(),
            text: None,
            keys: Vec::new(),
            confirm: false,
        };
        let call = plan_write(action).request.unwrap();
        assert_eq!(call.method, "POST");
        assert_eq!(call.target, "/v1/agents/w1:p9/seen");
        assert!(call.body.is_empty());
    }

    #[test]
    fn a_pane_id_that_is_not_a_pane_is_refused() {
        for pane in ["claude", "w1", "w1:", ":p1", "w1:p1:x", "w1:x1", "a:b"] {
            let plan = plan_write(respond(pane, "hola"));
            assert_eq!(
                plan.problem.unwrap().message,
                "Ese panel no es válido.",
                "pane {pane}"
            );
        }
        assert!(plan_write(respond("w1:p1", "hola")).request.is_some());
    }

    #[test]
    fn stop_is_a_post_with_no_body_and_a_validated_pane() {
        let call = plan_stop("w1:p4".to_string()).request.unwrap();
        assert_eq!(call.method, "POST");
        assert_eq!(call.target, "/v1/agents/w1:p4/stop");
        assert!(call.body.is_empty());

        assert_eq!(
            plan_stop("w1".to_string()).problem.unwrap().message,
            "Ese panel no es válido."
        );
    }
}
