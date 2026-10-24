//! Test-only signed HTTP client for the herdr-notify core.
//!
//! Adapted (minimal copy, with attribution) from the desktop client at
//! `desktop/core/src/{signing,identity,client}.rs` (MIT), which is the product
//! client. This crate exists so the core's integration tests and the manual
//! demo can speak the same signed protocol without depending on another
//! worktree's live code. It is a dev-dependency (`publish = false`) and is
//! never part of a shipped artifact; the canonical signing string itself is
//! reused from `hn-core-auth`, so server and client share one implementation.

mod approval_client;
pub mod client;
pub mod identity;
pub mod seat;

pub use client::{pair_device, HttpError, PairResponse, SignedClient};
pub use identity::DemoIdentity;
pub use seat::FakeSeat;
