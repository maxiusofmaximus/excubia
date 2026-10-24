//! The SSE stream: the opening full-state frame, change frames, keepalive
//! comments every 20 s, and the revocation check before every frame
//! (`docs/contracts/state.md` and `auth.md`).
//!
//! Revocation reaches an established stream two ways: immediately through the
//! in-process revocation broadcast (the daemon's own `revoke` route), and on
//! the next keepalive tick through `reload_if_changed` (a revocation performed
//! by the CLI in another process). Either way the stream ends, and nothing at
//! all flows to a revoked device.

use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::response::sse::Event;
use futures_core::Stream;
use hn_core_auth::{DeviceStore, StreamRegistry};
use hn_core_chat::ChatFrame;
use hn_core_state::SharedFrame;
use tokio::sync::{broadcast, mpsc};

const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(20);

/// What the forwarder sends into a stream.
pub(crate) enum StreamItem {
    Frame(SharedFrame),
    /// A durable chat frame. Its `id` is the global chat revision, which is
    /// what `Last-Event-ID` replays.
    Chat(ChatFrame),
    Keepalive,
    /// The device was revoked: end the stream without yielding anything more.
    Revoked,
}

pub struct EventStream {
    rx: mpsc::UnboundedReceiver<StreamItem>,
    first: Option<Event>,
    streams: Arc<StreamRegistry>,
    device_id: String,
}

impl EventStream {
    pub(crate) fn new(
        first: Event,
        rx: mpsc::UnboundedReceiver<StreamItem>,
        streams: Arc<StreamRegistry>,
        device_id: String,
    ) -> Self {
        Self {
            rx,
            first: Some(first),
            streams,
            device_id,
        }
    }
}

impl Stream for EventStream {
    type Item = Result<Event, std::convert::Infallible>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        if let Some(first) = self.first.take() {
            return Poll::Ready(Some(Ok(first)));
        }
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(StreamItem::Frame(frame))) => {
                let event = Event::default()
                    .event(frame.event.clone())
                    .data(frame.body.to_string());
                Poll::Ready(Some(Ok(event)))
            }
            Poll::Ready(Some(StreamItem::Chat(frame))) => {
                let event = Event::default()
                    .id(frame.id.to_string())
                    .event(frame.event)
                    .data(frame.body.to_string());
                Poll::Ready(Some(Ok(event)))
            }
            Poll::Ready(Some(StreamItem::Keepalive)) => {
                Poll::Ready(Some(Ok(Event::default().comment("keepalive"))))
            }
            Poll::Ready(Some(StreamItem::Revoked)) | Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for EventStream {
    fn drop(&mut self) {
        self.streams.release(&self.device_id);
    }
}

/// Everything one stream's forwarder needs. Grouped so the call site names
/// its parts instead of passing seven positional arguments.
pub(crate) struct Forwarder {
    pub frames: broadcast::Receiver<SharedFrame>,
    pub chat: broadcast::Receiver<ChatFrame>,
    /// Journal frames missed by a reconnecting client, oldest first. They are
    /// emitted before any live frame.
    pub replay: Vec<ChatFrame>,
    pub revocations: broadcast::Receiver<String>,
    pub devices: Arc<DeviceStore>,
    pub device_id: String,
    pub tx: mpsc::UnboundedSender<StreamItem>,
}

/// Forward state frames, chat frames, keepalives and revocations into one
/// stream channel. The task ends (and the stream closes) on revocation or when
/// the client goes away.
pub(crate) fn spawn_forwarder(forwarder: Forwarder) {
    tokio::spawn(run_forwarder(forwarder));
}

async fn run_forwarder(forwarder: Forwarder) {
    let Forwarder {
        mut frames,
        mut chat,
        replay,
        mut revocations,
        devices,
        device_id,
        tx,
    } = forwarder;

    // The subscription was opened before the journal was read, so a live chat
    // frame at or below this floor is already in the replay: sending it twice
    // would duplicate the client's timeline.
    let mut floor = 0;
    for frame in replay {
        // The contract checks trust before every frame a stream yields, and a
        // resume is no exception: a revoked device gets nothing, not even the
        // backlog it missed.
        if !still_trusted(&devices, &device_id) {
            let _ = tx.send(StreamItem::Revoked);
            return;
        }
        floor = frame.id;
        if tx.send(StreamItem::Chat(frame)).is_err() {
            return;
        }
    }

    let mut keepalive = tokio::time::interval(KEEPALIVE_INTERVAL);
    keepalive.tick().await; // the opening frame is not a keepalive
    loop {
        tokio::select! {
            item = frames.recv() => match item {
                Ok(frame) => {
                    if !still_trusted(&devices, &device_id) {
                        let _ = tx.send(StreamItem::Revoked);
                        break;
                    }
                    if tx.send(StreamItem::Frame(frame)).is_err() {
                        break;
                    }
                }
                // A lagging subscriber is dropped from the queue and
                // re-syncs from /v1/state (contract state.md).
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            item = chat.recv() => match item {
                Ok(frame) => {
                    if frame.id <= floor {
                        continue;
                    }
                    floor = frame.id;
                    if !still_trusted(&devices, &device_id) {
                        let _ = tx.send(StreamItem::Revoked);
                        break;
                    }
                    if tx.send(StreamItem::Chat(frame)).is_err() {
                        break;
                    }
                }
                // Missed chat frames are not replayed automatically mid-stream:
                // the frame tells the client to refresh the timeline anyway
                // (orchestrator-chat.md §3.2), which is the same resync rule.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            revoked = revocations.recv() => match revoked {
                Ok(revoked) if revoked == device_id => {
                    let _ = tx.send(StreamItem::Revoked);
                    break;
                }
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if !still_trusted(&devices, &device_id) {
                        let _ = tx.send(StreamItem::Revoked);
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            _ = keepalive.tick() => {
                if !still_trusted(&devices, &device_id) {
                    let _ = tx.send(StreamItem::Revoked);
                    break;
                }
                if tx.send(StreamItem::Keepalive).is_err() {
                    break;
                }
            }
        }
    }
}

/// The trust check, re-reading the store first so a CLI-side revocation is
/// picked up even when no other request has run since.
fn still_trusted(devices: &DeviceStore, device_id: &str) -> bool {
    devices.reload_if_changed();
    devices.get_trusted(device_id).is_some()
}
