//! Chat commands: the screen reads a reduced snapshot, never raw pages.
//!
//! Every command signs with the device identity and returns a typed
//! `ChatFailure`, so the webview picks a card by `kind` instead of parsing
//! status codes.

use std::sync::Arc;

use herdr_desktop_core::chat::{ChatFailure, ChatResult};
use herdr_desktop_core::{ChatSnapshot, HostClient};
use tauri::State;

use crate::app_state::{lock, AppState};

pub mod approvals;
pub mod attachments;
pub mod conversations;
pub mod delegation;
pub mod writes;

pub(crate) type Shared = Arc<AppState>;

pub(crate) fn shared(state: &State<'_, Shared>) -> Shared {
    state.inner().clone()
}

pub(crate) fn client(shared: &AppState) -> ChatResult<HostClient> {
    let settings = shared.settings().map_err(|message| ChatFailure::Protocol {
        message: format!("La configuración local no está lista: {message}"),
    })?;
    settings.client().map_err(|error| ChatFailure::Protocol {
        message: format!("No se pudo preparar la conexión: {error}"),
    })
}

fn snapshot(shared: &AppState) -> ChatSnapshot {
    lock(&shared.chat).snapshot()
}
