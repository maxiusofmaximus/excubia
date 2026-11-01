use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use herdr_desktop_core::chat::ChatEvent;
use herdr_desktop_core::state::StateSnapshot;
use herdr_desktop_core::stream::{run_event_loop, ConnectionStatus, EventSink, StreamOptions};
use herdr_desktop_core::TransitionTracker;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use super::{lock, AppState};

impl AppState {
    pub fn stop_stream(&self) {
        if let Some(cancel) = lock(&self.cancel).take() {
            cancel.cancel();
        }
    }

    pub fn start_stream(self: &Arc<Self>, app: AppHandle) -> Result<(), String> {
        let local_ready = {
            let inner = lock(&self.inner);
            !inner.config.local || inner.runtime_url.is_some()
        };
        if !local_ready {
            return Err("el host local todavía está arrancando".to_string());
        }
        let client = self
            .settings()?
            .client()
            .map_err(|error| error.to_string())?;
        let cancel = CancellationToken::new();
        if let Some(previous) = lock(&self.cancel).replace(cancel.clone()) {
            previous.cancel();
        }

        let store = self.store();
        let sink = Arc::new(TauriSink {
            app,
            shared: self.clone(),
            tracker: Mutex::new(TransitionTracker::default()),
        });
        tauri::async_runtime::spawn(async move {
            run_event_loop(
                Arc::new(client),
                store,
                sink,
                cancel,
                StreamOptions::default(),
            )
            .await;
        });
        Ok(())
    }
}

struct TauriSink {
    app: AppHandle,
    shared: Arc<AppState>,
    tracker: Mutex<TransitionTracker>,
}

#[async_trait]
impl EventSink for TauriSink {
    async fn connection(&self, status: ConnectionStatus) {
        lock(&self.shared.inner).connection = status.clone();
        let _ = self.app.emit("hn-connection", status);
    }

    async fn state(&self, state: StateSnapshot) {
        let alerts = lock(&self.tracker).observe(&state);
        let _ = self.app.emit("hn-state", state);
        if alerts.is_empty() || !lock(&self.shared.inner).config.notifications {
            return;
        }
        crate::notifications::show_alerts(&self.app, &alerts);
    }

    async fn chat(&self, event: ChatEvent) {
        let _ = self.app.emit("hn-chat", event);
    }
}
