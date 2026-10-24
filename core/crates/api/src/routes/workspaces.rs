//! `POST /v1/workspaces`: start a new conversation.
//!
//! The client names an agent from the host's closed list and a working
//! directory; it never names an executable. The response carries the pane id
//! and the initial agent record, mirroring the gateway's shape.
//!
//! A request that does not satisfy the model is rejected with a dedicated
//! `422` code and an explicit field (`errors.md`): `unknown_agent`,
//! `agent_unavailable`, `invalid_cwd`, `invalid_label`.

use std::path::PathBuf;

use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::routes::parse_body;
use crate::ApiContext;

/// The gateway caps a conversation name at 64 characters.
const MAX_LABEL_CHARS: usize = 64;

#[derive(Deserialize)]
struct NewWorkspace {
    agent: String,
    cwd: String,
    #[serde(default)]
    label: Option<String>,
}

/// The four model rejections this route can answer, each with its stable code
/// and the field the client should highlight.
enum Rejection {
    UnknownAgent { name: String, known: String },
    UnavailableAgent { name: String },
    InvalidCwd { message: String },
    InvalidLabel { message: String },
}

impl Rejection {
    fn into_api_error(self) -> ApiError {
        match self {
            Rejection::UnknownAgent { name, known } => ApiError::invalid_field(
                "unknown_agent",
                "agent",
                format!("unknown agent {name:?}; the host knows: {known}"),
            ),
            Rejection::UnavailableAgent { name } => ApiError::invalid_field(
                "agent_unavailable",
                "agent",
                format!("agent {name:?} is not installed on this host"),
            ),
            Rejection::InvalidCwd { message } => {
                ApiError::invalid_field("invalid_cwd", "cwd", message)
            }
            Rejection::InvalidLabel { message } => {
                ApiError::invalid_field("invalid_label", "label", message)
            }
        }
    }
}

pub async fn create(State(ctx): State<ApiContext>, body: Bytes) -> Response {
    let payload: NewWorkspace = match parse_body(&body) {
        Ok(payload) => payload,
        Err(error) => return error.into_response(),
    };

    let Some(spec) = ctx.agents.resolve(&payload.agent) else {
        let known = ctx.agents.names().join(", ");
        return Rejection::UnknownAgent {
            name: payload.agent,
            known,
        }
        .into_api_error()
        .into_response();
    };
    if !spec.available() {
        return Rejection::UnavailableAgent {
            name: payload.agent,
        }
        .into_api_error()
        .into_response();
    }
    let cwd = match normalize_cwd(&payload.cwd) {
        Ok(cwd) => cwd,
        Err(message) => {
            return Rejection::InvalidCwd { message }
                .into_api_error()
                .into_response()
        }
    };
    let label = match normalize_label(payload.label.as_deref()) {
        Ok(label) => label,
        Err(message) => {
            return Rejection::InvalidLabel { message }
                .into_api_error()
                .into_response()
        }
    };

    match ctx
        .actions
        .start_agent(&spec.command, &spec.args, &cwd, label.as_deref())
        .await
    {
        Ok(started) => Json(json!({
            "ok": true,
            "workspace_id": started.workspace_id,
            "pane_id": started.pane_id,
            "agent": {
                "agent": started.agent,
                "agent_status": started.status,
                "pane_id": started.pane_id,
            },
        }))
        .into_response(),
        // The host could not start it (pty failure, a command that vanished
        // between the check and the spawn): a legitimate refusal.
        Err(detail) => ApiError::refused(detail).into_response(),
    }
}

/// The working directory must exist and be a directory; the stored form is
/// canonical so two spellings of one path cannot look like two places.
fn normalize_cwd(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("cwd is required".to_string());
    }
    let path = std::fs::canonicalize(trimmed)
        .map_err(|error| format!("cwd {trimmed:?} does not exist: {error}"))?;
    if !path.is_dir() {
        return Err(format!("cwd {trimmed:?} is not a directory"));
    }
    Ok(strip_verbatim(path))
}

/// Windows `canonicalize` returns `\\?\C:\…`; strip the verbatim prefix for
/// display, keeping real UNC paths intact.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(text) = path.to_str() {
            if let Some(rest) = text.strip_prefix(r"\\?\") {
                if !rest.starts_with("UNC\\") {
                    return PathBuf::from(rest);
                }
            }
        }
    }
    path
}

fn normalize_label(label: Option<&str>) -> Result<Option<String>, String> {
    let Some(label) = label else {
        return Ok(None);
    };
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_LABEL_CHARS {
        return Err(format!("label cannot exceed {MAX_LABEL_CHARS} characters"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err("label cannot contain control characters".to_string());
    }
    Ok(Some(trimmed.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cwd_must_be_an_existing_directory() {
        let current = std::env::current_dir().expect("cwd");
        assert!(normalize_cwd(&current.to_string_lossy()).is_ok());
        assert!(normalize_cwd("").is_err());
        assert!(normalize_cwd("definitely/not/here").is_err());
        // A file is not a directory.
        let file = current.join("Cargo.toml");
        assert!(normalize_cwd(&file.to_string_lossy()).is_err());
    }

    #[test]
    fn labels_are_trimmed_and_bounded() {
        assert_eq!(normalize_label(None).expect("none"), None);
        assert_eq!(normalize_label(Some("   ")).expect("blank"), None);
        assert_eq!(
            normalize_label(Some("  mi chat  ")).expect("label"),
            Some("mi chat".to_string())
        );
        assert!(normalize_label(Some(&"a".repeat(65))).is_err());
        assert!(normalize_label(Some("dos\nlineas")).is_err());
    }

    #[test]
    fn verbatim_prefixes_are_stripped_for_display() {
        let stripped = strip_verbatim(PathBuf::from(r"\\?\C:\work"));
        assert_eq!(stripped, std::path::Path::new(r"C:\work"));
    }

    #[test]
    fn every_rejection_carries_its_code_and_field() {
        let cases = [
            (
                Rejection::UnknownAgent {
                    name: "x".into(),
                    known: "fake".into(),
                },
                "unknown_agent",
                "agent",
            ),
            (
                Rejection::UnavailableAgent { name: "x".into() },
                "agent_unavailable",
                "agent",
            ),
            (
                Rejection::InvalidCwd {
                    message: "no".into(),
                },
                "invalid_cwd",
                "cwd",
            ),
            (
                Rejection::InvalidLabel {
                    message: "no".into(),
                },
                "invalid_label",
                "label",
            ),
        ];
        for (rejection, code, field) in cases {
            let error = rejection.into_api_error();
            assert_eq!(error.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(error.body["detail"]["code"], code);
            assert_eq!(error.body["detail"]["field"], field);
            assert!(error.body["detail"]["message"].is_string());
        }
    }
}
