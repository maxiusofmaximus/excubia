//! SPAKE2 pairing core with key confirmation bound to the TLS channel (PAIRING-F2A, `docs/contracts/pairing.md`).
//!
//! The pairing code is the PAKE password. After one message each way both sides hold a key that matches only if they
//! used the same code and identities; each then sends a confirmation tag that covers the RFC 9266 TLS exporter and the
//! host's SPKI hash, so a relay with two TLS sessions is detected. The cryptography is the `spake2` crate (pinned) plus
//! HMAC-SHA256 from RustCrypto; nothing here is a new primitive.
//!
//! No network, no async and no host dependencies, so the Android and desktop clients can depend on it by path.
#![forbid(unsafe_code)]

mod code;
mod error;
mod key;
mod session;

pub use code::{Code, CODE_LEN};
pub use error::PakeError;
pub use key::SessionKey;
pub use session::{Role, Session, MESSAGE_LEN};
