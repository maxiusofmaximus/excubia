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

/// Which engine answers Alfred, read from the markers and from Strata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendStatus {
    /// `claude` or `local`: the engine Alfred's turns use.
    pub backend: &'static str,
    /// `claude`, `starting`, `local`, `stopping` or `failed`.
    pub state: &'static str,
    pub detail: Option<String>,
    /// Unix seconds: since the last change this host process saw.
    pub since: u64,
    /// True while the voice recognizer is down for the local model.
    pub voice_suspended: bool,
}

impl BackendStatus {
    pub fn to_json(&self) -> Value {
        json!({
            "backend": self.backend,
            "state": self.state,
            "detail": self.detail,
            "since": self.since,
            "voice_suspended": self.voice_suspended,
        })
    }
}

/// What the host does for Alfred's engine. The routes never touch Strata themselves.
#[async_trait]
pub trait AlfredBackend: Send + Sync + 'static {
    async fn status(&self) -> BackendStatus;

    /// `backend` is `local` or `claude`; the route has checked it. `Err` is a refusal, with the
    /// text for the phone: the other engine is being switched to right now.
    async fn request(&self, backend: &str) -> Result<Requested, String>;
}
