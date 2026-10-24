//! The HTTP error shapes of `docs/contracts/errors.md`, in one place so no
//! route invents its own.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// How long a client waits before it sends a turn the seat refused as busy.
const AGENT_BUSY_RETRY_AFTER_SECONDS: u64 = 5;

use hn_core_auth::{AuthzReason, Permission};

/// An API failure, already shaped for the wire.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub body: serde_json::Value,
    pub retry_after: Option<u64>,
}

impl ApiError {
    /// Every authentication rejection is one uniform answer: distinguishing
    /// them would turn the endpoint into an oracle for which devices exist.
    pub fn unauthorized(reason: impl std::fmt::Display) -> Self {
        eprintln!("auth: refused a signed request: {reason}");
        Self {
            status: StatusCode::UNAUTHORIZED,
            body: json!({"detail": "unauthorized"}),
            retry_after: None,
        }
    }

    /// Every authorization rejection is one uniform answer too; the reason
    /// goes to the log, which is the audit trail.
    pub fn forbidden(reason: AuthzReason, required: Option<Permission>, device: &str) -> Self {
        let required = required.map(|permission| permission.label()).unwrap_or("-");
        eprintln!("auth: refused {device}: {reason:?} (required: {required})");
        Self {
            status: StatusCode::FORBIDDEN,
            body: json!({"detail": "forbidden"}),
            retry_after: None,
        }
    }

    /// `409 agent_busy`: the seat's queue has no room now. The turn was not
    /// delivered; the client waits `Retry-After` seconds and sends it again.
    pub fn agent_busy() -> Self {
        Self {
            status: StatusCode::CONFLICT,
            body: json!({"detail": "agent_busy"}),
            retry_after: Some(AGENT_BUSY_RETRY_AFTER_SECONDS),
        }
    }

    pub fn slow_down(retry_after: u64) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            body: json!({"detail": "slow down"}),
            retry_after: Some(retry_after),
        }
    }

    pub fn too_many_streams() -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            body: json!({"detail": "too many open streams"}),
            retry_after: Some(20),
        }
    }

    pub fn body_too_large() -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            body: json!({"detail": "body too large"}),
            retry_after: None,
        }
    }

    pub fn invalid_body() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: json!({"detail": "invalid_body"}),
            retry_after: None,
        }
    }

    /// A signed body that is JSON but not the expected model.
    pub fn invalid_body_model(message: String) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            body: json!({"detail": {"code": "invalid_body", "errors": [{"loc": [], "msg": message}]}}),
            retry_after: None,
        }
    }

    /// A model field that is JSON-valid but semantically invalid (`cwd` that
    /// is not a directory, an agent name outside the host's list).
    pub fn invalid_body_field(field: &str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            body: json!({
                "detail": {
                    "code": "invalid_body",
                    "errors": [{"loc": [field], "msg": message.into()}],
                }
            }),
            retry_after: None,
        }
    }

    /// A dedicated model rejection: a stable `code`, the `field` the client
    /// should highlight, and a short message (`errors.md`).
    pub fn invalid_field(code: &str, field: &str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            body: json!({
                "detail": {
                    "code": code,
                    "field": field,
                    "message": message.into(),
                }
            }),
            retry_after: None,
        }
    }

    /// The pane is no longer known: its process is gone and the pane was
    /// removed from the state list (`errors.md`, `pane_gone`).
    pub fn pane_gone(pane_id: &str) -> Self {
        Self::action("pane_gone", format!("pane {pane_id} is no longer known"))
    }

    /// An action failure, `400` with a stable code from `errors.md`.
    pub fn action(code: &str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            body: json!({"detail": {"code": code, "message": message.into()}}),
            retry_after: None,
        }
    }

    /// A known resource the host does not have (`errors.md`: `404` for an
    /// unknown or foreign id).
    pub fn not_found(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            body: json!({"detail": detail.into()}),
            retry_after: None,
        }
    }

    pub fn no_such_device() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            body: json!({"detail": "no such device"}),
            retry_after: None,
        }
    }

    /// A hook credential that exists but is not allowed to do this: the token
    /// of a closed run, the host token on an approval route, or the token of
    /// another run. It is typed because the hook must fail closed and the
    /// operator must be able to tell it from a bad signature.
    pub fn forbidden_code(code: &str) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            body: json!({"detail": code}),
            retry_after: None,
        }
    }

    /// The host declined or could not deliver an action: a legitimate answer,
    /// not a fault (the `409` of `errors.md`).
    pub fn refused(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            body: json!({"detail": detail.into()}),
            retry_after: None,
        }
    }

    pub fn internal(detail: impl std::fmt::Display) -> Self {
        eprintln!("api: internal error: {detail}");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            body: json!({"detail": "internal error"}),
            retry_after: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self.body)).into_response();
        if let Some(retry_after) = self.retry_after {
            if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}
