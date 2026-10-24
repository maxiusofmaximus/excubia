//! Rust client core of the herdr notify Android app.
//!
//! Flutter reaches this crate through flutter_rust_bridge, via the [`api`]
//! module. It holds the pairing-link parser and the pinned TLS client, request
//! signing, the SSE stream and the wire format of the host state.

pub mod api;
mod frb_generated;
mod pinned;
mod sse;
mod state_wire;
