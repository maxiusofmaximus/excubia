//! The signed HTTP client used by tests and the manual demo.
//!
//! Adapted from `desktop/core/src/client.rs` (MIT): same headers, same target
//! (`path?query` exactly as signed), same body hashing. The canonical string
//! is not copied â€” it is `excubia_auth::canonical_string`, the same function
//! the server verifies with.

mod api;
mod events;
mod pair;
mod signed;

pub use pair::{pair_device, PairResponse};
pub(crate) use signed::signed_query;
pub use signed::{HttpError, SignedClient};
