//! Typed chat failures: the only place a status code becomes a card kind.

use reqwest::{Response, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::client::status_error;
use crate::error::{CoreError, WriteAction};

pub type ChatResult<T> = Result<T, ChatFailure>;

/// A `chat.*` frame of the SSE stream. It never carries a body: the shell
/// refreshes the affected timeline, which is the authoritative view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatFailure {
    Offline {
        message: String,
    },
    Unauthorized {
        message: String,
        action: Option<String>,
    },
    Forbidden {
        message: String,
        action: String,
    },
    SeatUnavailable {
        message: String,
        action: String,
    },
    AgentBusy {
        message: String,
        retry_after_secs: Option<u64>,
        status: u16,
    },
    Archived {
        message: String,
    },
    NotFound {
        message: String,
    },
    /// A typed delegation refusal (`depth_exceeded`, `pane_limit`, …). The
    /// code is kept so the card and its detail can name the rule.
    DelegationRefused {
        code: String,
        message: String,
        status: u16,
    },
    /// The host already settled the approval (decided elsewhere, expired,
    /// consumed). The card reads the host again; its state is never guessed.
    ApprovalConflict {
        code: String,
        message: String,
    },
    Invalid {
        message: String,
        field: Option<String>,
    },
    Gateway {
        message: String,
        status: u16,
    },
    Protocol {
        message: String,
    },
}

impl ChatFailure {
    pub fn from_core(error: &CoreError, write: bool) -> Self {
        let action = if write {
            WriteAction::Respond
        } else {
            WriteAction::Read
        };
        if let CoreError::Status {
            status,
            code,
            field,
            message,
        } = error
        {
            let code = code.as_deref();
            return match status {
                404 => ChatFailure::NotFound {
                    message: "Esa conversación ya no existe en el host.".to_string(),
                },
                409 => match code.unwrap_or(message).trim() {
                    "seat_unavailable" => ChatFailure::SeatUnavailable {
                        message: "El host no encuentra un CLI con MCP. El historial se conserva y no se perdió ningún mensaje."
                            .to_string(),
                        action: "Revisa el motor y el CLI en el equipo del host.".to_string(),
                    },
                    "agent_busy" => ChatFailure::AgentBusy {
                        message: "Tu mensaje no se envió. Reinténtalo cuando termine el turno actual."
                            .to_string(),
                        retry_after_secs: None,
                        status: *status,
                    },
                    "conversation_archived" => ChatFailure::Archived {
                        message: "Esta conversación está archivada. Desarchívala para escribir."
                            .to_string(),
                    },
                    other if delegation_text(other).is_some() => ChatFailure::DelegationRefused {
                        code: other.to_string(),
                        message: delegation_text(other).unwrap_or_default().to_string(),
                        status: *status,
                    },
                    other => ChatFailure::Gateway {
                        message: format!("El host rechazó la petición: {other}"),
                        status: *status,
                    },
                },
                422 => {
                    let info = error.describe(action);
                    ChatFailure::Invalid {
                        message: info.message,
                        field: field.clone(),
                    }
                }
                429 => ChatFailure::AgentBusy {
                    message: "El host está limitando las peticiones. Espera y reinténtalo."
                        .to_string(),
                    retry_after_secs: None,
                    status: *status,
                },
                _ => ChatFailure::Gateway {
                    message: error.describe(action).text(),
                    status: *status,
                },
            };
        }
        match error {
            CoreError::Unauthorized => {
                let info = error.describe(action);
                ChatFailure::Unauthorized {
                    message: info.message,
                    action: info.action,
                }
            }
            CoreError::Forbidden => {
                let info = error.describe(action);
                ChatFailure::Forbidden {
                    message: info.message,
                    action: info.action.unwrap_or_default(),
                }
            }
            CoreError::Transport(_) => ChatFailure::Offline {
                message: "Sin conexión con el host. Lo cargado sigue siendo legible.".to_string(),
            },
            CoreError::InvalidInput(message) => ChatFailure::Invalid {
                message: message.clone(),
                field: None,
            },
            other => ChatFailure::Protocol {
                message: other.to_string(),
            },
        }
    }
}

pub(super) async fn decode<T: DeserializeOwned>(response: Response, write: bool) -> ChatResult<T> {
    let status = response.status();
    let retry_after_secs = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    let bytes = response
        .bytes()
        .await
        .map_err(|error| ChatFailure::from_core(&CoreError::from(error), write))?;

    if status.is_success() {
        return serde_json::from_slice(&bytes).map_err(protocol);
    }

    Err(status_failure(status, &bytes, retry_after_secs, write))
}

pub(super) fn status_failure(
    status: StatusCode,
    body: &[u8],
    retry_after_secs: Option<u64>,
    write: bool,
) -> ChatFailure {
    let detail = detail_code(body);
    if status == StatusCode::FORBIDDEN {
        let info = CoreError::Forbidden.describe(if write {
            WriteAction::Respond
        } else {
            WriteAction::Read
        });
        return ChatFailure::Forbidden {
            message: info.message,
            action: info.action.unwrap_or_default(),
        };
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return ChatFailure::AgentBusy {
            message: "El host está limitando las peticiones. Tu mensaje no se envió.".to_string(),
            retry_after_secs,
            status: status.as_u16(),
        };
    }
    if status == StatusCode::NOT_FOUND && detail.as_deref() == Some("no_such_delegation") {
        return ChatFailure::NotFound {
            message: "Esa delegación ya no existe en el host.".to_string(),
        };
    }
    if status == StatusCode::CONFLICT {
        return match detail.as_deref().unwrap_or("") {
            "seat_unavailable" => ChatFailure::SeatUnavailable {
                message: "El host no encuentra un CLI con MCP. El historial se conserva y no se perdió ningún mensaje."
                    .to_string(),
                action: "Revisa el motor y el CLI en el equipo del host.".to_string(),
            },
            "agent_busy" => ChatFailure::AgentBusy {
                message: "Tu mensaje no se envió. Reinténtalo cuando termine el turno actual."
                    .to_string(),
                retry_after_secs,
                status: status.as_u16(),
            },
            "conversation_archived" => ChatFailure::Archived {
                message: "Esta conversación está archivada. Desarchívala para escribir.".to_string(),
            },
            code if delegation_text(code).is_some() => ChatFailure::DelegationRefused {
                code: code.to_string(),
                message: delegation_text(code).unwrap_or_default().to_string(),
                status: status.as_u16(),
            },
            code if approval_text(code).is_some() => ChatFailure::ApprovalConflict {
                code: code.to_string(),
                message: approval_text(code).unwrap_or_default().to_string(),
            },
            _ => ChatFailure::Gateway {
                message: error_message(body),
                status: status.as_u16(),
            },
        };
    }
    ChatFailure::from_core(&status_error(status, body), write)
}

/// The rule behind a delegation refusal, in the language of the operator. A
/// `None` means the code is not a delegation rule at all.
pub fn delegation_text(code: &str) -> Option<&'static str> {
    Some(match code {
        "delegation_cancelled" => "La delegación ya había terminado o estaba cancelada.",
        "depth_exceeded" => {
            "La delegación supera la profundidad permitida (Alfred → lead → especialista)."
        }
        "pane_limit" => "El host ya tiene seis paneles vivos; no se puede abrir otro.",
        "cycle_detected" => {
            "La delegación repite un encargo reciente; se detuvo para evitar un ciclo."
        }
        "turn_budget" => "Este turno ya hizo doce delegaciones; no se permiten más.",
        "hour_budget" => "Este asiento ya hizo sesenta delegaciones en la última hora.",
        "seat_limit" => "El proyecto ya tiene ocho asientos; no cabe otro.",
        "scope_denied" => "Ese asiento no puede hacer esta delegación.",
        "lead_exists" => "El proyecto ya tiene un lead.",
        "project_exists" => "Ya existe un proyecto con ese nombre.",
        "not_your_delegation" => "Esa delegación pertenece a otro asiento.",
        "unknown_project" => "Ese proyecto no existe en el host.",
        "unknown_agent" => "Ese asiento no existe en el host.",
        "no_such_delegation" => "Esa delegación ya no existe en el host.",
        "delegation_timeout" => "El turno de la delegación agotó el tiempo de espera.",
        "pane_gone" => "El panel del asignado desapareció antes de terminar.",
        "host_restart" => "El host se reinició y la delegación quedó interrumpida.",
        "conversation_cancelled" => "Se canceló el equipo de esta conversación.",
        "user_cancelled" => "La delegación se canceló desde el escritorio.",
        _ => return None,
    })
}

/// What an approval conflict means to the operator. A `None` means the code is
/// not an approval rule at all.
pub fn approval_text(code: &str) -> Option<&'static str> {
    Some(match code {
        "approval_already_decided" => {
            "Otro dispositivo ya decidió esta aprobación. La tarjeta se ha actualizado."
        }
        "approval_expired" => "Se agotó el tiempo de esta aprobación: no se ejecutó.",
        "approval_digest_reused" => {
            "Esa acción ya se pidió antes: el host no abre otra aprobación."
        }
        "approval_mismatch" => "La acción cambió después de pedir permiso: no se ejecutó.",
        "approval_not_approved" => "Esta aprobación no está permitida: no se puede usar.",
        "approval_limit" => "La conversación ya tiene cuatro aprobaciones esperando.",
        _ => return None,
    })
}

fn detail_code(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    match value.get("detail") {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Object(fields)) => fields
            .get("code")
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

fn error_message(body: &[u8]) -> String {
    let value: Value = serde_json::from_slice(body).ok().unwrap_or(Value::Null);
    match value.get("detail") {
        Some(Value::String(text)) => format!("El host rechazó la petición: {text}"),
        Some(Value::Object(fields)) => fields
            .get("message")
            .and_then(Value::as_str)
            .map(|text| format!("El host rechazó la petición: {text}"))
            .unwrap_or_else(|| "El host rechazó la petición.".to_string()),
        _ => "El host rechazó la petición.".to_string(),
    }
}

pub(crate) fn protocol(error: impl std::fmt::Display) -> ChatFailure {
    ChatFailure::Protocol {
        message: format!("respuesta inesperada del host: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conflict(code: &str) -> ChatFailure {
        let body = format!(r#"{{"ok":false,"detail":{{"code":"{code}"}}}}"#);
        status_failure(StatusCode::CONFLICT, body.as_bytes(), None, true)
    }

    #[test]
    fn a_decision_the_host_already_settled_is_an_approval_conflict() {
        match conflict("approval_already_decided") {
            ChatFailure::ApprovalConflict { code, message } => {
                assert_eq!(code, "approval_already_decided");
                assert!(message.contains("Otro dispositivo"));
            }
            other => panic!("se esperaba ApprovalConflict, llegó {other:?}"),
        }
    }

    #[test]
    fn an_expired_approval_names_that_nothing_ran() {
        match conflict("approval_expired") {
            ChatFailure::ApprovalConflict { message, .. } => {
                assert!(message.contains("no se ejecutó"))
            }
            other => panic!("se esperaba ApprovalConflict, llegó {other:?}"),
        }
    }

    #[test]
    fn an_unknown_conflict_stays_a_gateway_refusal() {
        assert!(matches!(
            conflict("made_up_code"),
            ChatFailure::Gateway { status: 409, .. }
        ));
    }
}
