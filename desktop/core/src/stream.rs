use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::chat::ChatEvent;
use crate::client::GatewayClient;
use crate::sse::{SseMessage, SseParser};
use crate::state::{StateSnapshot, StateStore};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ConnectionStatus {
    Idle,
    Connecting,
    Connected { revision: u64 },
    Offline { reason: String },
    Fatal { reason: String },
}

/// What the shell draws. The core never knows about windows, events or HTML.
#[async_trait]
pub trait EventSink: Send + Sync {
    async fn connection(&self, status: ConnectionStatus);
    async fn state(&self, state: StateSnapshot);
    /// Chat frames do not change the agent state and are not replayed as
    /// state; a shell that ignores them still sees the authoritative timeline
    /// when it opens it.
    async fn chat(&self, _event: ChatEvent) {}
}

#[derive(Debug, Clone)]
pub struct StreamOptions {
    pub retry_base: Duration,
    pub retry_max: Duration,
    pub idle_timeout: Duration,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            retry_base: Duration::from_secs(1),
            retry_max: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(45),
        }
    }
}

/// Connects, decodes events, re-reads `/v1/state` on change frames, and
/// reconnects with exponential backoff until cancelled or the device is refused.
pub async fn run_event_loop(
    client: Arc<GatewayClient>,
    store: Arc<Mutex<StateStore>>,
    sink: Arc<dyn EventSink>,
    cancel: CancellationToken,
    options: StreamOptions,
) {
    let mut delay = options.retry_base;

    loop {
        if cancel.is_cancelled() {
            return;
        }
        sink.connection(ConnectionStatus::Connecting).await;

        let response = match client.events().await {
            Ok(response) => response,
            Err(error) => {
                if error.is_fatal_for_stream() {
                    sink.connection(ConnectionStatus::Fatal {
                        reason: fatal_reason(&error),
                    })
                    .await;
                    return;
                }
                sink.connection(ConnectionStatus::Offline {
                    reason: error.to_string(),
                })
                .await;
                if !sleep_or_cancel(&cancel, delay).await {
                    return;
                }
                delay = next_delay(delay, options.retry_max);
                continue;
            }
        };

        delay = options.retry_base;
        let revision = store
            .lock()
            .ok()
            .and_then(|store| store.revision())
            .unwrap_or(0);
        sink.connection(ConnectionStatus::Connected { revision })
            .await;

        let mut stream = response.bytes_stream();
        let mut parser = SseParser::new();
        let mut idle = Box::pin(tokio::time::sleep(options.idle_timeout));

        let reason = loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = &mut idle => {
                    break "El gateway dejó de enviar datos.".to_string();
                }
                item = stream.next() => {
                    match item {
                        Some(Ok(chunk)) => {
                            idle.as_mut().reset(tokio::time::Instant::now() + options.idle_timeout);
                            for message in parser.feed(&chunk) {
                                match handle_message(&client, &store, &sink, message).await {
                                    FrameAction::Continue => {}
                                    FrameAction::Fatal(reason) => {
                                        sink.connection(ConnectionStatus::Fatal { reason }).await;
                                        return;
                                    }
                                }
                            }
                        }
                        Some(Err(error)) => {
                            break format!("Se perdió la conexión: {error}");
                        }
                        None => {
                            break "El gateway cerró la conexión.".to_string();
                        }
                    }
                }
            }
        };

        sink.connection(ConnectionStatus::Offline { reason }).await;

        if !sleep_or_cancel(&cancel, delay).await {
            return;
        }
        delay = next_delay(delay, options.retry_max);
    }
}

enum FrameAction {
    Continue,
    Fatal(String),
}

async fn handle_message(
    client: &GatewayClient,
    store: &Mutex<StateStore>,
    sink: &Arc<dyn EventSink>,
    message: SseMessage,
) -> FrameAction {
    let Ok(value) = serde_json::from_str::<Value>(&message.data) else {
        return FrameAction::Continue;
    };
    let event = message.event.as_deref().unwrap_or("change");

    if event == "state" {
        if let Some(raw_state) = value.get("state") {
            if let Ok(state) = serde_json::from_value::<StateSnapshot>(raw_state.clone()) {
                emit_if_applied(store, sink, state).await;
            }
        }
        return FrameAction::Continue;
    }

    // Chat revisions are a different counter than the state revision: they
    // refresh the timeline, not `/v1/state`.
    if event.starts_with("chat.") {
        sink.chat(ChatEvent::from_frame(event, &value)).await;
        return FrameAction::Continue;
    }

    if let Some(incoming) = value.get("revision").and_then(Value::as_u64) {
        let current = store
            .lock()
            .ok()
            .and_then(|store| store.revision())
            .unwrap_or(0);
        if incoming <= current {
            return FrameAction::Continue;
        }
    }

    match client.state().await {
        Ok(state) => {
            emit_if_applied(store, sink, state).await;
            FrameAction::Continue
        }
        Err(error) if error.is_fatal_for_stream() => FrameAction::Fatal(fatal_reason(&error)),
        Err(_) => FrameAction::Continue,
    }
}

async fn emit_if_applied(
    store: &Mutex<StateStore>,
    sink: &Arc<dyn EventSink>,
    state: StateSnapshot,
) {
    let applied = store.lock().ok().and_then(|mut store| store.apply(state));
    if let Some(state) = applied {
        sink.state(state).await;
    }
}

fn fatal_reason(error: &crate::error::CoreError) -> String {
    match error {
        crate::error::CoreError::Unauthorized => {
            "El gateway ya no confía en este dispositivo. Vuelve a emparejarlo.".to_string()
        }
        crate::error::CoreError::Forbidden => {
            "Este dispositivo no tiene permiso para leer el estado.".to_string()
        }
        other => other.to_string(),
    }
}

async fn sleep_or_cancel(cancel: &CancellationToken, delay: Duration) -> bool {
    tokio::select! {
        _ = cancel.cancelled() => false,
        _ = tokio::time::sleep(delay) => true,
    }
}

fn next_delay(current: Duration, maximum: Duration) -> Duration {
    current.saturating_mul(2).min(maximum)
}
