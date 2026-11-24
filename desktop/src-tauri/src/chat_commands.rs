//! Chat commands: the screen reads a reduced snapshot, never raw pages.
//!
//! Every command signs with the device identity and returns a typed
//! `ChatFailure`, so the webview picks a card by `kind` instead of parsing
//! status codes.

use std::sync::Arc;

use excubia_desktop_core::chat::{ChatFailure, ChatResult};
use excubia_desktop_core::ui_lang::pick;
use excubia_desktop_core::{ChatSnapshot, HostClient};
use tauri::State;

use crate::app_state::{lock, AppState};

pub mod approvals;
pub mod attachments;
pub mod conversations;
pub mod delegation;
pub mod documents;
pub mod links;
pub mod preview;
pub mod vault;
pub mod writes;

pub(crate) type Shared = Arc<AppState>;

pub(crate) fn shared(state: &State<'_, Shared>) -> Shared {
    state.inner().clone()
}

pub(crate) fn client(shared: &AppState) -> ChatResult<HostClient> {
    let settings = shared.settings().map_err(|message| ChatFailure::Protocol {
        message: format!(
            "{}: {message}",
            pick(
                "La configuración local no está lista",
                "The local setup is not ready",
            )
        ),
    })?;
    settings.client().map_err(|error| ChatFailure::Protocol {
        message: format!(
            "{}: {error}",
            pick(
                "No se pudo preparar la conexión",
                "The connection could not be prepared",
            )
        ),
    })
}

fn snapshot(shared: &AppState) -> ChatSnapshot {
    lock(&shared.chat).snapshot()
}
