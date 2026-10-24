//! `POST /v1/workspaces`: start one agent by name in a host folder.

use serde_json::json;

use super::error::{info, FailureKind};
use super::write::{HttpCall, WritePlan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedWorkspace {
    pub workspace_id: String,
    pub pane_id: String,
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_created(json: String) -> Option<CreatedWorkspace> {
    let value: serde_json::Value = serde_json::from_str(&json).ok()?;
    if !value.get("ok")?.as_bool()? {
        return None;
    }
    let pane_id = value.get("pane_id")?.as_str()?.trim().to_string();
    if pane_id.is_empty() {
        return None;
    }
    let workspace_id = value
        .get("workspace_id")
        .and_then(|id| id.as_str())
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| pane_id.split(':').next().unwrap_or_default().to_string());
    Some(CreatedWorkspace {
        workspace_id,
        pane_id,
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn plan_create(agent: String, cwd: String, label: Option<String>) -> WritePlan {
    let agent = agent.trim();
    if agent.is_empty() {
        return problem(
            "Elige un agente del catálogo.",
            Some("Pulsa actualizar si la lista está vacía."),
        );
    }
    let cwd = cwd.trim();
    if cwd.is_empty() {
        return problem(
            "Indica la carpeta de trabajo.",
            Some("Es una ruta del equipo que ejecuta el host, no del teléfono."),
        );
    }
    if cwd.chars().any(char::is_control) {
        return problem("Esa carpeta no es válida.", None);
    }
    let label = label
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty());
    if let Some(label) = &label {
        if label.chars().count() > 64 {
            return problem(
                "La etiqueta es demasiado larga.",
                Some("Usa 64 caracteres como máximo."),
            );
        }
        if label.chars().any(char::is_control) {
            return problem("La etiqueta no es válida.", None);
        }
    }

    let mut body = json!({ "agent": agent, "cwd": cwd });
    if let Some(label) = label {
        body["label"] = serde_json::Value::String(label);
    }
    match serde_json::to_vec(&body) {
        Ok(bytes) => WritePlan {
            request: Some(HttpCall {
                method: "POST".to_string(),
                target: "/v1/workspaces".to_string(),
                body: bytes,
            }),
            problem: None,
        },
        Err(_) => problem("La petición no es válida.", None),
    }
}

fn problem(message: &str, action: Option<&str>) -> WritePlan {
    WritePlan {
        request: None,
        problem: Some(info(message, action, FailureKind::Retry)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_create_plan_carries_the_name_the_folder_and_the_label() {
        let plan = plan_create(
            " fake ".to_string(),
            " D:\\work ".to_string(),
            Some(" mi chat ".to_string()),
        );
        assert!(plan.problem.is_none());
        let call = plan.request.unwrap();
        assert_eq!(call.method, "POST");
        assert_eq!(call.target, "/v1/workspaces");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&call.body).unwrap(),
            serde_json::json!({
                "agent": "fake",
                "cwd": "D:\\work",
                "label": "mi chat",
            })
        );
    }

    #[test]
    fn a_blank_label_is_omitted() {
        let plan = plan_create(
            "fake".to_string(),
            "D:\\work".to_string(),
            Some("   ".to_string()),
        );
        let call = plan.request.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
        assert!(body.get("label").is_none());
    }

    #[test]
    fn the_local_validation_names_the_field() {
        assert_eq!(
            plan_create(" ".to_string(), "D:\\work".to_string(), None)
                .problem
                .unwrap()
                .message,
            "Elige un agente del catálogo."
        );
        assert_eq!(
            plan_create("fake".to_string(), "  ".to_string(), None)
                .problem
                .unwrap()
                .message,
            "Indica la carpeta de trabajo."
        );
        assert_eq!(
            plan_create(
                "fake".to_string(),
                "D:\\work".to_string(),
                Some("a".repeat(65))
            )
            .problem
            .unwrap()
            .message,
            "La etiqueta es demasiado larga."
        );
        assert_eq!(
            plan_create(
                "fake".to_string(),
                "D:\\work".to_string(),
                Some("dos\nlineas".to_string())
            )
            .problem
            .unwrap()
            .message,
            "La etiqueta no es válida."
        );
    }

    #[test]
    fn parses_the_created_workspace() {
        let created = parse_created(
            r#"{"ok":true,"workspace_id":"w1","pane_id":"w1:p4",
                "agent":{"agent":"fake","agent_status":"unknown","pane_id":"w1:p4"}}"#
                .to_string(),
        )
        .unwrap();
        assert_eq!(created.workspace_id, "w1");
        assert_eq!(created.pane_id, "w1:p4");
    }

    #[test]
    fn a_created_payload_without_a_pane_is_not_a_conversation() {
        assert!(parse_created(r#"{"ok":true,"workspace_id":"w1"}"#.to_string()).is_none());
        assert!(parse_created(r#"{"ok":false,"pane_id":"w1:p1"}"#.to_string()).is_none());
        assert!(parse_created("not json".to_string()).is_none());

        let created = parse_created(r#"{"ok":true,"pane_id":"w2:p9"}"#.to_string()).unwrap();
        assert_eq!(created.workspace_id, "w2");
    }
}
