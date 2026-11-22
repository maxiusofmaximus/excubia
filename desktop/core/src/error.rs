use serde::Serialize;
use thiserror::Error;

use crate::transport::tls_failure;

mod host;
#[cfg(test)]
mod tests;

use host::{
    bad_request_info, create_refused_info, forbidden_info, invalid_field_info, refusal_info,
    stop_refused_info,
};

/// Errors the shell can present without knowing which transport failed.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("no se pudo contactar con el PC: {0}")]
    Transport(String),

    #[error("el PC respondió {status}: {message}")]
    Status {
        status: u16,
        code: Option<String>,
        /// The `loc[0]` of a `422` body, e.g. `agent`, `cwd` or `label`.
        field: Option<String>,
        message: String,
    },

    #[error("el PC ya no confía en este dispositivo; vuelve a emparejarlo")]
    Unauthorized,

    #[error("este dispositivo no tiene permiso para esa operación")]
    Forbidden,

    #[error("{0}")]
    PairingRejected(String),

    #[error("respuesta inesperada del PC: {0}")]
    Protocol(String),

    #[error("dato inválido: {0}")]
    InvalidInput(String),

    #[error("no se pudo usar el almacén seguro: {0}")]
    SecureStore(String),

    #[error("la huella del host no coincide")]
    PinMismatch,

    #[error("el certificado del host ha caducado")]
    CertificateExpired,

    #[error("el certificado del host aún no es válido")]
    CertificateNotYetValid,

    #[error("el enlace de emparejamiento ha caducado")]
    LinkExpired,

    #[error("enlace de emparejamiento no válido: {0}")]
    LinkInvalid(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

impl From<reqwest::Error> for CoreError {
    fn from(value: reqwest::Error) -> Self {
        tls_failure(&value).unwrap_or_else(|| CoreError::Transport(value.to_string()))
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(value: serde_json::Error) -> Self {
        CoreError::Protocol(value.to_string())
    }
}

/// The write that failed. A uniform `403` does not say which permission is
/// missing, so the caller's intent is what lets the message name the grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteAction {
    Respond,
    Keys,
    Seen,
    Read,
    CreateWorkspace,
    Stop,
    SetDefault,
}

/// The canonical host-side grant command (CORE4's `GRANT_HINT`).
pub const GRANT_HINT: &str = "host devices grant <device_id> --permission <read_state|respond|send_key|approve|admin> [--data-dir DIR]";

/// One message plus the action that resolves it. The only place a host code
/// becomes text, so the UI never branches on status codes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorInfo {
    pub message: String,
    pub action: Option<String>,
}

impl ErrorInfo {
    pub fn text(&self) -> String {
        match &self.action {
            Some(action) => format!("{} {}", self.message, action),
            None => self.message.clone(),
        }
    }
}

impl CoreError {
    pub fn is_fatal_for_stream(&self) -> bool {
        matches!(self, CoreError::Unauthorized | CoreError::Forbidden)
    }

    /// The text a pairing screen shows: what happened, then what to do next.
    pub fn pairing_message(&self) -> String {
        let info = self.describe(WriteAction::Read);
        match info.action {
            Some(action) => format!("{} {action}", info.message),
            None => info.message,
        }
    }

    pub fn describe(&self, action: WriteAction) -> ErrorInfo {
        match self {
            CoreError::Unauthorized => ErrorInfo {
                message: "El PC ya no confía en este dispositivo.".to_string(),
                action: Some("Vuelve a emparejarlo desde Avanzado.".to_string()),
            },
            CoreError::Forbidden => forbidden_info(action),
            CoreError::Status {
                status: 422, field, ..
            } => invalid_field_info(field.as_deref()),
            CoreError::Status {
                status: 409,
                message,
                ..
            } => match action {
                WriteAction::CreateWorkspace => create_refused_info(message),
                WriteAction::Stop => stop_refused_info(message),
                _ => refusal_info(message),
            },
            CoreError::Status { status: 429, .. } => ErrorInfo {
                message: "El PC está limitando las peticiones.".to_string(),
                action: Some("Espera unos segundos y vuelve a intentarlo.".to_string()),
            },
            CoreError::Status {
                status: 400,
                code,
                message,
                ..
            } => bad_request_info(code.as_deref(), message),
            CoreError::Status { status: 413, .. } => ErrorInfo {
                message: "El mensaje es demasiado largo para el PC.".to_string(),
                action: Some("Acórtalo y vuelve a intentarlo.".to_string()),
            },
            CoreError::Status {
                status, message, ..
            } if *status >= 500 => ErrorInfo {
                message: format!("El PC tuvo un fallo interno ({status}: {message})."),
                action: Some("Vuelve a intentarlo en unos segundos.".to_string()),
            },
            CoreError::Status {
                status, message, ..
            } => ErrorInfo {
                message: format!("El PC respondió {status}: {message}"),
                action: Some("Vuelve a intentarlo.".to_string()),
            },
            CoreError::Transport(_) => ErrorInfo {
                message: "Sin conexión con el PC.".to_string(),
                action: Some("Comprueba la red y vuelve a intentarlo.".to_string()),
            },
            CoreError::PairingRejected(message) => ErrorInfo {
                message: message.clone(),
                action: None,
            },
            CoreError::Protocol(message) => ErrorInfo {
                message: format!("Respuesta inesperada del PC: {message}"),
                action: None,
            },
            CoreError::InvalidInput(message) => ErrorInfo {
                message: message.clone(),
                action: None,
            },
            CoreError::SecureStore(message) => ErrorInfo {
                message: message.clone(),
                action: None,
            },
            CoreError::PinMismatch => ErrorInfo {
                message: "La huella del host no coincide.".to_string(),
                action: Some(
                    "No te conectes a esta red. Si el host cambió, genera un enlace nuevo."
                        .to_string(),
                ),
            },
            CoreError::CertificateExpired => ErrorInfo {
                message: "El certificado del host ha caducado.".to_string(),
                action: Some("Genera un enlace nuevo en el host.".to_string()),
            },
            CoreError::CertificateNotYetValid => ErrorInfo {
                message: "El certificado del host aún no es válido.".to_string(),
                action: Some("Revisa la hora de este equipo y vuelve a intentarlo.".to_string()),
            },
            CoreError::LinkExpired => ErrorInfo {
                message: "El enlace de emparejamiento ha caducado.".to_string(),
                action: Some("Genera uno nuevo en el host.".to_string()),
            },
            CoreError::LinkInvalid(message) => ErrorInfo {
                message: format!("El enlace de emparejamiento no es válido: {message}."),
                action: None,
            },
        }
    }
}
