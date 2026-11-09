//! The model that answers Alfred, as the phone reads and changes it (`docs/contracts/alfred-backend.md`).
//! The host runs the switch: it starts and stops the local model and writes its markers. This
//! module only holds the shape of the answers and the trait the routes call.

use async_trait::async_trait;
use serde_json::{json, Value};

/// What a request to change the engine came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requested {
    /// The engine is the one asked for already: `200`.
    Unchanged,
    /// The change runs in the background, or is already running: `202`.
    Started,
}

/// A CLI engine the host can run, listed only once it is `Ready` (installed and its gate proven).
/// Alfred may choose it; its turns then run on that CLI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineListing {
    pub name: String,
    pub version: String,
}

/// Which engine answers Alfred, read from the markers and from Strata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendStatus {
    /// `claude`, `local`, or the name of a listed engine (e.g. `opencode`).
    pub backend: String,
    /// `claude`, `starting`, `local`, `engine`, `stopping` or `failed`.
    pub state: &'static str,
    pub detail: Option<String>,
    /// Unix seconds: since the last change this host process saw.
    pub since: u64,
    /// True while the voice recognizer is down for the local model.
    pub voice_suspended: bool,
    /// CLI engines that are `Ready` on this PC. Empty when none is.
    pub engines: Vec<EngineListing>,
}

impl BackendStatus {
    pub fn to_json(&self) -> Value {
        json!({
            "backend": self.backend,
            "state": self.state,
            "detail": self.detail,
            "since": self.since,
            "voice_suspended": self.voice_suspended,
            "engines": self
                .engines
                .iter()
                .map(|engine| json!({ "name": engine.name, "version": engine.version }))
                .collect::<Vec<_>>(),
        })
    }
}

/// What the host does for Alfred's engine. The routes never touch Strata themselves.
#[async_trait]
pub trait AlfredBackend: Send + Sync + 'static {
    async fn status(&self) -> BackendStatus;

    /// `backend` is `local`, `claude` or a listed engine; the route has checked it. `Err` is a
    /// refusal, with the text for the phone: the other engine is being switched to right now.
    async fn request(&self, backend: &str) -> Result<Requested, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(engines: Vec<EngineListing>) -> BackendStatus {
        BackendStatus {
            backend: "claude".to_owned(),
            state: "claude",
            detail: None,
            since: 0,
            voice_suspended: false,
            engines,
        }
    }

    #[test]
    fn no_ready_engine_lists_an_empty_array() {
        assert_eq!(status(Vec::new()).to_json()["engines"], json!([]));
    }

    #[test]
    fn a_ready_engine_is_listed_by_name_and_version() {
        let listed = status(vec![EngineListing {
            name: "opencode".to_owned(),
            version: "2.2.6".to_owned(),
        }]);
        assert_eq!(
            listed.to_json()["engines"],
            json!([{ "name": "opencode", "version": "2.2.6" }])
        );
    }
}
