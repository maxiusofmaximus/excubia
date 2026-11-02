//! Mail through the host (`/v1/mail`, MAIL2A): accounts, sign-in by device
//! code, the draft the operator reads, and the approved send. The wire shapes and
//! the typed failures live here; the signed calls are in `client`.

mod approval;
mod client;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use approval::{MailApprovalDetails, MailMoveItem};

pub type MailResult<T> = Result<T, MailFailure>;

/// A refused or failed mail call. `code` is the contract's code, or `forbidden` and
/// `unauthorized` for the two statuses that carry none. `message` is already in
/// Spanish, so the screen never shows a raw code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MailFailure {
    pub code: String,
    pub message: String,
    /// Seconds from the `Retry-After` header, on a rate limit only.
    pub retry_after_secs: Option<u64>,
}

impl MailFailure {
    pub fn coded(code: &str, retry_after_secs: Option<u64>) -> Self {
        Self {
            code: code.to_string(),
            message: message_for(code, retry_after_secs),
            retry_after_secs,
        }
    }
}

fn message_for(code: &str, retry_after_secs: Option<u64>) -> String {
    match (code, retry_after_secs) {
        ("mail_send_rate_limited", Some(seconds)) => format!(
            "{} Vuelve a intentarlo en {} min.",
            mail_text(code).unwrap_or_default(),
            seconds.div_ceil(60).max(1)
        ),
        _ => mail_text(code)
            .unwrap_or("El host no pudo completar la operación de correo.")
            .to_string(),
    }
}

/// What the operator reads for each code. `None` means the code is not one of
/// this contract's, so the caller falls back to a generic code.
pub fn mail_text(code: &str) -> Option<&'static str> {
    Some(match code {
        "mail_not_configured" => "El correo aún no está configurado en tu equipo. Falta registrar la aplicación de Microsoft; quien administra el equipo puede hacerlo.",
        "mail_invalid_input" => "Los datos enviados no son válidos. Revísalos y vuelve a intentarlo.",
        "mail_invalid_config" => "La configuración de correo del host no es válida. Quien administra el equipo debe revisarla.",
        "mail_unavailable" => "Microsoft o la red no responden. Reinténtalo más tarde.",
        "mail_auth_failed" => "Microsoft rechazó la sesión de esta cuenta. Vuelve a conectarla.",
        "mail_auth_required" => "Esta cuenta necesita volver a conectarse.",
        "mail_login_expired" => "El código caducó antes de usarlo. Empieza de nuevo.",
        "mail_login_denied" => "No se completó el inicio de sesión. Empieza de nuevo si quieres conectar la cuenta.",
        "mail_admin_consent_required" => "La organización de esta cuenta exige que un administrador apruebe la aplicación.",
        "mail_app_not_registered" => "Microsoft no reconoce la aplicación de correo de este equipo. Revisa su registro.",
        "mail_graph_error" => "Microsoft devolvió un error al leer o enviar el correo.",
        "mail_bad_response" => "El host respondió algo que no podemos leer.",
        "mail_unknown_draft" => "Ese borrador no lo creó este equipo, así que no se puede enviar desde aquí.",
        "mail_use_upload_session" => "El adjunto es demasiado grande para subirlo de una vez.",
        "mail_path_rejected" => "La ruta del adjunto no es válida.",
        "mail_approval_denied" => "El envío no se aprobó, o la aprobación caducó. No se envió nada.",
        "mail_approval_unavailable" => "No se pudo pedir la aprobación al host. Reinténtalo luego.",
        "mail_content_changed" => "El archivo cambió después de aprobarlo. No se envió nada.",
        "mail_upload_failed" => "No se pudo subir el adjunto. El borrador no cambió.",
        "mail_store_error" => "El almacén de cuentas del host falló. Quien administra el equipo debe revisarlo.",
        "mail_send_not_enabled" => "Esta cuenta no tiene permiso para enviar. Activa «Permitir enviar» en Ajustes → Correo.",
        "mail_approval_stale" => "La aprobación ya se usó o el borrador cambió después de aprobarlo. No se envió nada: pide la aprobación de nuevo.",
        "mail_approval_pending" => "Todavía esperando tu aprobación.",
        "mail_send_rate_limited" => "Se alcanzó el límite de envíos por hora.",
        "local_setup" => "La configuración local de este equipo no está lista. Reinicia la aplicación.",
        "forbidden" => "Este equipo no tiene permiso para gestionar el correo. Quien administra el equipo puede concedérselo.",
        "unauthorized" => "El host no reconoce este equipo. Vuelve a emparejarlo.",
        _ => return None,
    })
}

/// The failure of a non-2xx answer. Only the contract's own codes reach the
/// screen; anything else becomes a generic code chosen by the status.
pub(crate) fn failure_from_status(
    status: StatusCode,
    body: &[u8],
    retry_after_secs: Option<u64>,
) -> MailFailure {
    if status == StatusCode::FORBIDDEN {
        return MailFailure::coded("forbidden", None);
    }
    if status == StatusCode::UNAUTHORIZED {
        return MailFailure::coded("unauthorized", None);
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return MailFailure::coded("mail_send_rate_limited", retry_after_secs);
    }
    match detail_code(body).filter(|code| mail_text(code).is_some()) {
        Some(code) => MailFailure::coded(&code, retry_after_secs),
        None if status.is_client_error() => MailFailure::coded("mail_invalid_input", None),
        None if status.is_server_error() => MailFailure::coded("mail_unavailable", None),
        None => MailFailure::coded("mail_bad_response", None),
    }
}

fn detail_code(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    value
        .get("detail")?
        .get("code")?
        .as_str()
        .map(str::to_string)
}

/// One account of the store. `send_enabled` is true only with the send consent.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MailAccount {
    pub label: String,
    #[serde(default)]
    pub tenant: String,
    #[serde(default)]
    pub upn: Option<String>,
    #[serde(default)]
    pub send_enabled: bool,
}

/// The device-code sign-in the operator completes at `verification_uri`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceLogin {
    pub login_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    #[serde(default = "poll_interval")]
    pub interval: u64,
}

fn poll_interval() -> u64 {
    5
}

/// The state of the last sign-in of one label. A failed state carries its text,
/// filled in by `localized` from the code the host sent.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LoginStatus {
    Pending,
    Ok {
        label: String,
        #[serde(default)]
        upn: Option<String>,
    },
    Denied {
        #[serde(default)]
        message: String,
    },
    Expired {
        #[serde(default)]
        message: String,
    },
    Error {
        code: String,
        #[serde(default)]
        message: String,
    },
}

impl LoginStatus {
    pub(crate) fn localized(self) -> Self {
        match self {
            Self::Denied { .. } => Self::Denied {
                message: mail_text("mail_login_denied")
                    .unwrap_or_default()
                    .to_string(),
            },
            Self::Expired { .. } => Self::Expired {
                message: mail_text("mail_login_expired")
                    .unwrap_or_default()
                    .to_string(),
            },
            Self::Error { code, .. } => {
                let message = message_for(&code, None);
                Self::Error { code, message }
            }
            other => other,
        }
    }
}

/// A draft in the operator's mailbox. The body is text, never markup.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MailDraft {
    pub account: String,
    pub draft_id: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub content_type: String,
    #[serde(default)]
    pub attachments: Vec<MailAttachment>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct MailAttachment {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub sha256: String,
}

/// The answer to asking to send. Without an approval the host opens one and
/// answers `awaiting_approval`; with it, the approval is spent and the send is
/// scheduled for `window_seconds`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SendRequest {
    AwaitingApproval {
        approval_id: String,
        expires_in: u64,
    },
    Pending {
        send_id: String,
        window_seconds: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SendPhase {
    Pending,
    Sent,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SendState {
    pub send_id: String,
    pub state: SendPhase,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SendToggled {
    pub account: String,
    pub send_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_contract_code_has_a_spanish_text() {
        for code in [
            "mail_not_configured",
            "mail_invalid_input",
            "mail_unavailable",
            "mail_auth_failed",
            "mail_login_expired",
            "mail_login_denied",
            "mail_send_not_enabled",
            "mail_approval_stale",
            "mail_approval_denied",
            "mail_unknown_draft",
            "mail_send_rate_limited",
        ] {
            let text = mail_text(code).expect(code);
            assert!(
                !text.is_empty() && !text.contains("mail_"),
                "{code}: {text}"
            );
        }
    }

    #[test]
    fn a_rate_limit_says_how_long_to_wait_in_minutes() {
        let failure = failure_from_status(StatusCode::TOO_MANY_REQUESTS, b"", Some(1500));
        assert_eq!(failure.code, "mail_send_rate_limited");
        assert_eq!(failure.retry_after_secs, Some(1500));
        assert!(
            failure.message.ends_with("en 25 min."),
            "{}",
            failure.message
        );
    }

    #[test]
    fn a_known_contract_code_keeps_its_text() {
        let body = br#"{"detail":{"code":"mail_send_not_enabled","message":"x"}}"#;
        let failure = failure_from_status(StatusCode::BAD_REQUEST, body, None);
        assert_eq!(failure.code, "mail_send_not_enabled");
        assert!(failure.message.contains("Ajustes → Correo"));
    }

    #[test]
    fn an_unknown_code_never_reaches_the_screen_raw() {
        let body = br#"{"detail":{"code":"zz_secret_path_/tmp/x"}}"#;
        let failure = failure_from_status(StatusCode::BAD_REQUEST, body, None);
        assert_eq!(failure.code, "mail_invalid_input");
        assert!(!failure.message.contains("zz_"));
    }

    #[test]
    fn a_forbidden_answer_is_forbidden_whatever_the_body() {
        let failure =
            failure_from_status(StatusCode::FORBIDDEN, br#"{"detail":"forbidden"}"#, None);
        assert_eq!(failure.code, "forbidden");
    }

    #[test]
    fn a_send_request_tells_awaiting_from_pending() {
        let awaiting: SendRequest = serde_json::from_value(serde_json::json!({
            "ok": true, "state": "awaiting_approval", "approval_id": "apr_1", "expires_in": 120
        }))
        .expect("awaiting");
        assert!(matches!(
            awaiting,
            SendRequest::AwaitingApproval {
                expires_in: 120,
                ..
            }
        ));
        let pending: SendRequest = serde_json::from_value(serde_json::json!({
            "ok": true, "state": "pending", "send_id": "snd_1", "window_seconds": 10
        }))
        .expect("pending");
        assert!(matches!(
            pending,
            SendRequest::Pending {
                window_seconds: 10,
                ..
            }
        ));
    }
}
