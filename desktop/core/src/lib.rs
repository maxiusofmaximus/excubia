//! UI-free client core for herdr notify.
//!
//! The crate knows the gateway contracts (pairing, signed reads, SSE) and
//! nothing about Tauri. The desktop shell uses it directly; an Android shell
//! can reuse it through JNI/uniffi and supply its own `IdentityStore`.

pub mod chat;
pub mod chat_feed;
pub mod client;
pub mod error;
pub mod identity;
pub mod notify;
pub mod pair_link;
pub mod signing;
pub mod sse;
pub mod state;
pub mod stream;
pub mod transport;

pub use chat::{
    ChatBootstrap, ChatEvent, ChatFailure, Conversation, ConversationPatch, Message, Section,
    SectionChange, TimelineEntry,
};
pub use chat_feed::{ChatEntry, ChatFeed, ChatSnapshot};
pub use client::{CatalogAgent, GatewayClient, PairResponse, WorkspaceCreated};
pub use error::{CoreError, ErrorInfo, Result, WriteAction};
pub use hn_core_pin::{Fingerprint, PinSet};
pub use identity::{
    default_device_label, normalize_device_label, validate_device_label, DeviceIdentity,
    IdentityStore, MAX_DEVICE_LABEL_CHARS,
};
pub use notify::{AgentAlert, TransitionTracker};
pub use pair_link::{pair_through_link, LinkPairing, PairLink};
pub use state::{AgentRow, AgentStatus, StateSnapshot, StateStore};
pub use stream::{run_event_loop, ConnectionStatus, EventSink, StreamOptions};
pub use transport::Transport;
