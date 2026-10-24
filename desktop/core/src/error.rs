use serde::Serialize;
use thiserror::Error;

use crate::transport::tls_failure;

/// Errors the shell can present without knowing which transport failed.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("no se pudo contactar con el gateway: {0}")]
    Transport(String),

    #[error("el gateway respondió {status}: {message}")]
    Status {
        status: u16,
        code: Option<String>,
        /// The `loc[0]` of a `422` body, e.g. `agent`, `cwd` or `label`.
        field: Option<String>,
        message: String,
    },

    #[error("el gateway ya no confía en este dispositivo; vuelve a emparejarlo")]
    Unauthorized,

    #[error("este dispositivo no tiene permiso para esa operación")]
    Forbidden,

    #[error("{0}")]
    PairingRejected(String),

    #[error("respuesta inesperada del gateway: {0}")]
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
}

/// The canonical host-side grant command (CORE4's `GRANT_HINT`).
pub const GRANT_HINT: &str = "host devices grant <device_id> --permission <read_state|respond|send_key|approve|admin> [--data-dir DIR]";

/// One message plus the action that resolves it. The only place a gateway code
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
                message: "El gateway ya no confía en este dispositivo.".to_string(),
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
                message: "El gateway está limitando las peticiones.".to_string(),
                action: Some("Espera unos segundos y vuelve a intentarlo.".to_string()),
            },
            CoreError::Status {
                status: 400,
                code,
                message,
                ..
            } => bad_request_info(code.as_deref(), message),
            CoreError::Status { status: 413, .. } => ErrorInfo {
                message: "El mensaje es demasiado largo para el gateway.".to_string(),
                action: Some("Acórtalo y vuelve a intentarlo.".to_string()),
            },
            CoreError::Status {
                status, message, ..
            } if *status >= 500 => ErrorInfo {
                message: format!("El gateway tuvo un fallo interno ({status}: {message})."),
                action: Some("Vuelve a intentarlo en unos segundos.".to_string()),
            },
            CoreError::Status {
                status, message, ..
            } => ErrorInfo {
                message: format!("El gateway respondió {status}: {message}"),
                action: Some("Vuelve a intentarlo.".to_string()),
            },
            CoreError::Transport(_) => ErrorInfo {
                message: "Sin conexión con el gateway.".to_string(),
                action: Some("Comprueba la red y vuelve a intentarlo.".to_string()),
            },
            CoreError::PairingRejected(message) => ErrorInfo {
                message: message.clone(),
                action: None,
            },
            CoreError::Protocol(message) => ErrorInfo {
                message: format!("Respuesta inesperada del gateway: {message}"),
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

fn forbidden_info(action: WriteAction) -> ErrorInfo {
    let message = match action {
        WriteAction::Respond | WriteAction::CreateWorkspace | WriteAction::Stop => {
            "Este equipo solo puede leer; falta el permiso RESPOND."
        }
        WriteAction::Keys => "Este equipo solo puede leer; falta el permiso SEND_KEY.",
        WriteAction::Seen | WriteAction::Read => "Este equipo no tiene el permiso READ_STATE.",
    };
    ErrorInfo {
        message: message.to_string(),
        action: Some(format!("Concédelo en el host: {GRANT_HINT}")),
    }
}

fn invalid_field_info(field: Option<&str>) -> ErrorInfo {
    let (message, action) = match field {
        Some("agent") => (
            "Ese agente no está en el catálogo del host.",
            Some("Elige uno de la lista."),
        ),
        Some("cwd") => (
            "La carpeta no existe o no es un directorio.",
            Some("Elige otra carpeta."),
        ),
        Some("label") => (
            "La etiqueta no es válida (máximo 64 caracteres, sin caracteres de control).",
            Some("Cámbiala y vuelve a intentarlo."),
        ),
        _ => (
            "La petición no es válida.",
            Some("Revisa los datos y vuelve a intentarlo."),
        ),
    };
    ErrorInfo {
        message: message.to_string(),
        action: action.map(str::to_string),
    }
}

fn create_refused_info(message: &str) -> ErrorInfo {
    ErrorInfo {
        message: format!("El host no pudo iniciar el agente: {message}"),
        action: Some("Revisa el comando del agente en agents.toml.".to_string()),
    }
}

fn stop_refused_info(message: &str) -> ErrorInfo {
    ErrorInfo {
        message: format!("No se puede detener este agente: {message}"),
        action: Some("Inicia una nueva conversación.".to_string()),
    }
}

fn dead_panel_info() -> ErrorInfo {
    ErrorInfo {
        message: "Ese panel ya no existe.".to_string(),
        action: Some("Actualiza la lista y elige otro agente.".to_string()),
    }
}

fn refusal_info(message: &str) -> ErrorInfo {
    if message.contains("not found") || message.contains("pane_gone") {
        dead_panel_info()
    } else {
        ErrorInfo {
            message: format!("El agente no aceptó la acción: {message}"),
            action: Some("Actualiza el estado y vuelve a intentarlo.".to_string()),
        }
    }
}

fn bad_request_info(code: Option<&str>, message: &str) -> ErrorInfo {
    let (text, action) = match code {
        Some("empty_prompt") => (
            "El mensaje está vacío.",
            Some("Escribe algo antes de enviar."),
        ),
        Some("control_characters") => (
            "El mensaje contiene caracteres de control.",
            Some("Quita los caracteres invisibles y vuelve a enviarlo."),
        ),
        Some("empty_key" | "no_keys") => ("No hay ninguna tecla que enviar.", None),
        Some("too_many_keys") => ("Demasiadas teclas en una sola acción.", None),
        Some("confirmation_required") => (
            "Esa tecla cambia trabajo en curso y necesita confirmación.",
            None,
        ),
        Some("key_not_allowed") => ("Esa tecla no está permitida.", None),
        Some("pane_gone") => return dead_panel_info(),
        Some("bad_pane") if message.contains("no longer known") => return dead_panel_info(),
        Some("bad_pane") => (
            "Ese panel no es válido.",
            Some("Actualiza la lista y elige otro agente."),
        ),
        Some("invalid_body") => ("La petición no es válida.", None),
        _ => {
            return ErrorInfo {
                message: format!("El gateway rechazó la petición: {message}"),
                action: None,
            }
        }
    };
    ErrorInfo {
        message: text.to_string(),
        action: action.map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(status: u16, code: Option<&str>, message: &str) -> CoreError {
        CoreError::Status {
            status,
            code: code.map(str::to_string),
            field: None,
            message: message.to_string(),
        }
    }

    fn invalid_field(field: &str) -> CoreError {
        CoreError::Status {
            status: 422,
            code: Some("invalid_body".to_string()),
            field: Some(field.to_string()),
            message: "campo inválido".to_string(),
        }
    }

    #[test]
    fn a_403_names_the_missing_grant_and_the_host_command() {
        let info = CoreError::Forbidden.describe(WriteAction::Respond);
        assert!(info.message.contains("RESPOND"), "{}", info.message);
        assert_eq!(
            info.action.as_deref(),
            Some(format!("Concédelo en el host: {GRANT_HINT}").as_str())
        );

        let keys = CoreError::Forbidden.describe(WriteAction::Keys);
        assert!(keys.message.contains("SEND_KEY"));
        assert!(keys.action.as_deref().unwrap().contains(GRANT_HINT));

        let read = CoreError::Forbidden.describe(WriteAction::Read);
        assert!(read.message.contains("READ_STATE"));

        let stop = CoreError::Forbidden.describe(WriteAction::Stop);
        assert!(stop.message.contains("RESPOND"));
        assert!(stop.action.as_deref().unwrap().contains(GRANT_HINT));
    }

    #[test]
    fn the_canonical_grant_hint_is_the_core_one() {
        assert_eq!(
            GRANT_HINT,
            "host devices grant <device_id> --permission <read_state|respond|send_key|approve|admin> [--data-dir DIR]"
        );
    }

    #[test]
    fn a_409_on_stop_explains_the_agent_is_not_running() {
        let info = status(409, None, "no running agent").describe(WriteAction::Stop);
        assert!(
            info.message.contains("No se puede detener"),
            "{}",
            info.message
        );
        assert!(info
            .action
            .as_deref()
            .unwrap()
            .contains("nueva conversación"));
    }

    #[test]
    fn a_409_for_a_dead_pane_says_so_and_offers_an_action() {
        let info = status(409, None, "agent.respond: agent target w1:p1 not found")
            .describe(WriteAction::Respond);
        assert_eq!(info.message, "Ese panel ya no existe.");
        assert!(info.action.is_some());
    }

    #[test]
    fn pane_gone_is_the_same_dead_panel_as_a_409() {
        let info =
            status(400, Some("pane_gone"), "pane w1:p1 is gone").describe(WriteAction::Respond);
        assert_eq!(info.message, "Ese panel ya no existe.");
        assert!(info.action.is_some());
    }

    #[test]
    fn a_core_bad_pane_for_a_dead_pane_is_a_dead_panel() {
        let info = status(400, Some("bad_pane"), "pane w1:p1 is no longer known")
            .describe(WriteAction::Respond);
        assert_eq!(info.message, "Ese panel ya no existe.");
    }

    #[test]
    fn a_malformed_pane_stays_a_bad_pane() {
        let info = status(400, Some("bad_pane"), "\"claude\" is not a pane id")
            .describe(WriteAction::Respond);
        assert_eq!(info.message, "Ese panel no es válido.");
        assert!(info.action.is_some());
    }

    #[test]
    fn other_409s_are_refusals_not_faults() {
        let info = status(409, None, "agent is no longer blocked").describe(WriteAction::Keys);
        assert!(info.message.contains("no aceptó la acción"));
        assert!(info.action.is_some());
    }

    #[test]
    fn a_429_asks_to_wait() {
        let info = status(429, None, "slow down").describe(WriteAction::Respond);
        assert!(info.message.contains("limitando"));
        assert!(info.action.as_deref().unwrap().contains("Espera"));
    }

    #[test]
    fn action_codes_become_spanish_text() {
        let empty =
            status(400, Some("empty_prompt"), "text is required").describe(WriteAction::Respond);
        assert_eq!(empty.message, "El mensaje está vacío.");

        let key = status(400, Some("key_not_allowed"), "x is not an allowed key")
            .describe(WriteAction::Keys);
        assert_eq!(key.message, "Esa tecla no está permitida.");

        let confirmation =
            status(400, Some("confirmation_required"), "needs confirm").describe(WriteAction::Keys);
        assert!(confirmation.message.contains("confirmación"));
    }

    #[test]
    fn a_422_names_the_field_that_failed() {
        let agent = invalid_field("agent").describe(WriteAction::CreateWorkspace);
        assert!(agent.message.contains("catálogo"), "{}", agent.message);
        assert!(agent.action.is_some());

        let cwd = invalid_field("cwd").describe(WriteAction::CreateWorkspace);
        assert!(cwd.message.contains("carpeta"));

        let label = invalid_field("label").describe(WriteAction::CreateWorkspace);
        assert!(label.message.contains("etiqueta"));
    }

    #[test]
    fn creating_a_conversation_maps_403_and_409_with_its_own_text() {
        let forbidden = CoreError::Forbidden.describe(WriteAction::CreateWorkspace);
        assert!(forbidden.message.contains("RESPOND"));
        assert!(forbidden.action.as_deref().unwrap().contains(GRANT_HINT));

        let refused = status(409, None, "command not found").describe(WriteAction::CreateWorkspace);
        assert!(refused.message.contains("no pudo iniciar"));
        assert!(refused.action.as_deref().unwrap().contains("agents.toml"));
    }

    #[test]
    fn a_dead_network_and_a_dead_pairing_read_differently() {
        let offline =
            CoreError::Transport("connection refused".to_string()).describe(WriteAction::Respond);
        assert!(offline.message.contains("Sin conexión"));

        let revoked = CoreError::Unauthorized.describe(WriteAction::Respond);
        assert!(revoked.message.contains("ya no confía"));
        assert!(revoked.action.as_deref().unwrap().contains("emparejarlo"));
    }

    #[test]
    fn info_text_joins_the_message_and_the_action() {
        let info = CoreError::Forbidden.describe(WriteAction::Respond);
        let text = info.text();
        assert!(text.starts_with("Este equipo solo puede leer"));
        assert!(text.contains(GRANT_HINT), "{text}");
        assert!(!text.contains("herdr-notify devices"), "{text}");
    }

    #[test]
    fn a_pin_mismatch_tells_the_user_not_to_connect() {
        assert_eq!(
            CoreError::PinMismatch.pairing_message(),
            "La huella del host no coincide. No te conectes a esta red. Si el host cambió, genera un enlace nuevo."
        );
        assert_eq!(
            CoreError::LinkExpired.pairing_message(),
            "El enlace de emparejamiento ha caducado. Genera uno nuevo en el host."
        );
    }
}
