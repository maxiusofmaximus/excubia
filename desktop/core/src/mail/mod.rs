//! Mail through the host (`/v1/mail`, MAIL2A): accounts, sign-in by device
//! code, the draft the operator reads, and the approved send. The wire shapes and
//! the typed failures live here; the signed calls are in `client`.

mod approval;
mod client;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ui_lang::{self, UiLang};

pub use approval::{MailApprovalDetails, MailMoveItem};

pub type MailResult<T> = Result<T, MailFailure>;

/// A refused or failed mail call. `code` is the contract's code, or `forbidden` and
/// `unauthorized` for the two statuses that carry none. `message` is already in
/// the operator's language (see `ui_lang`), so the screen never shows a raw code.
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
        ("mail_send_rate_limited", Some(seconds)) => {
            let minutes = seconds.div_ceil(60).max(1);
            let text = mail_text(code).unwrap_or_default();
            if ui_lang::current().is_en() {
                format!("{text} Try again in {minutes} min.")
            } else {
                format!("{text} Vuelve a intentarlo en {minutes} min.")
            }
        }
        _ => mail_text(code)
            .unwrap_or_else(|| {
                ui_lang::pick(
                    "El host no pudo completar la operación de correo.",
                    "The host could not complete the mail operation.",
                )
            })
            .to_string(),
    }
}

/// What the operator reads for each code. `None` means the code is not one of
/// this contract's, so the caller falls back to a generic code.
pub fn mail_text(code: &str) -> Option<&'static str> {
    mail_text_in(code, ui_lang::current())
}

pub fn mail_text_in(code: &str, lang: UiLang) -> Option<&'static str> {
    Some(match code {
        "mail_not_configured" => lang.pick("El correo aún no está configurado en tu equipo. Falta registrar la aplicación de Microsoft; quien administra el equipo puede hacerlo.", "Mail is not set up on your PC yet. The Microsoft app registration is missing; whoever manages this PC can do it."),
        "mail_invalid_input" => lang.pick("Los datos enviados no son válidos. Revísalos y vuelve a intentarlo.", "The data sent is not valid. Check it and try again."),
        "mail_invalid_config" => lang.pick("La configuración de correo del host no es válida. Quien administra el equipo debe revisarla.", "The host's mail settings are not valid. Whoever manages this PC should check them."),
        "mail_unavailable" => lang.pick("Microsoft o la red no responden. Reinténtalo más tarde.", "Microsoft or the network is not responding. Try again later."),
        "mail_auth_failed" => lang.pick("Microsoft rechazó la sesión de esta cuenta. Vuelve a conectarla.", "Microsoft rejected this account's session. Connect it again."),
        "mail_auth_required" => lang.pick("Esta cuenta necesita volver a conectarse.", "This account needs to connect again."),
        "mail_login_expired" => lang.pick("El código caducó antes de usarlo. Empieza de nuevo.", "The code expired before it was used. Start again."),
        "mail_login_denied" => lang.pick("No se completó el inicio de sesión. Empieza de nuevo si quieres conectar la cuenta.", "The sign-in was not completed. Start again if you want to connect the account."),
        "mail_admin_consent_required" => lang.pick("La organización de esta cuenta exige que un administrador apruebe la aplicación.", "This account's organization requires an administrator to approve the app."),
        "mail_app_not_registered" => lang.pick("Microsoft no reconoce la aplicación de correo de este equipo. Revisa su registro.", "Microsoft does not recognize this PC's mail app. Check its registration."),
        "mail_graph_error" => lang.pick("Microsoft devolvió un error al leer o enviar el correo.", "Microsoft returned an error while reading or sending the mail."),
        "mail_bad_response" => lang.pick("El host respondió algo que no podemos leer.", "The host replied with something we cannot read."),
        "mail_unknown_draft" => lang.pick("Ese borrador no lo creó este equipo, así que no se puede enviar desde aquí.", "This PC did not create that draft, so it cannot be sent from here."),
        "mail_use_upload_session" => lang.pick("El adjunto es demasiado grande para subirlo de una vez.", "The attachment is too large to upload at once."),
        "mail_path_rejected" => lang.pick("La ruta del adjunto no es válida.", "The attachment path is not valid."),
        "mail_approval_denied" => lang.pick("El envío no se aprobó, o la aprobación caducó. No se envió nada.", "The send was not approved, or the approval expired. Nothing was sent."),
        "mail_approval_unavailable" => lang.pick("No se pudo pedir la aprobación al host. Reinténtalo luego.", "The approval could not be requested from the host. Try again later."),
        "mail_content_changed" => lang.pick("El archivo cambió después de aprobarlo. No se envió nada.", "The file changed after it was approved. Nothing was sent."),
        "mail_upload_failed" => lang.pick("No se pudo subir el adjunto. El borrador no cambió.", "The attachment could not be uploaded. The draft did not change."),
        "mail_store_error" => lang.pick("El almacén de cuentas del host falló. Quien administra el equipo debe revisarlo.", "The host's account store failed. Whoever manages this PC should check it."),
        "mail_send_not_enabled" => lang.pick("Esta cuenta no tiene permiso para enviar. Activa «Permitir enviar» en Ajustes → Correo.", "This account is not allowed to send. Turn on \"Allow sending\" in Settings → Email."),
        "mail_approval_stale" => lang.pick("La aprobación ya se usó o el borrador cambió después de aprobarlo. No se envió nada: pide la aprobación de nuevo.", "The approval was already used, or the draft changed after it was approved. Nothing was sent: ask for approval again."),
        "mail_approval_pending" => lang.pick("Todavía esperando tu aprobación.", "Still waiting for your approval."),
        "mail_send_rate_limited" => lang.pick("Se alcanzó el límite de envíos por hora.", "The hourly send limit was reached."),
        "local_setup" => lang.pick("La configuración local de este equipo no está lista. Reinicia la aplicación.", "This PC's local setup is not ready. Restart the app."),
        "forbidden" => lang.pick("Este equipo no tiene permiso para gestionar el correo. Quien administra el equipo puede concedérselo.", "This PC does not have permission to manage mail. Whoever manages this PC can grant it."),
        "unauthorized" => lang.pick("El host no reconoce este equipo. Vuelve a emparejarlo.", "The host does not recognize this PC. Pair it again."),
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
