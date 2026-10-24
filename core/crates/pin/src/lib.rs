//! The host's TLS identity and the pin rule that devices verify it with
//! (`docs/contracts/pairing.md`, CORE11).
//!
//! The host's certificate is self-signed and trusted only by the fingerprint
//! of its SubjectPublicKeyInfo, which reaches the device inside the pairing
//! link. Nothing here authenticates a request: the device key still signs
//! every call, and the transport is interchangeable.
//!
//! This crate has no host dependencies, so the Android and desktop clients
//! can depend on it by path.

mod certificate;
mod error;
mod fingerprint;
mod identity;
mod link;
mod pins;
mod tls;
mod verifier;

pub use certificate::CertificateInfo;
pub use error::PinError;
pub use fingerprint::Fingerprint;
pub use identity::{unix_now, Identity, DEFAULT_VALIDITY_SECONDS};
pub use link::{PairingLink, LINK_PREFIX, LINK_VERSION};
pub use pins::PinSet;
pub use tls::{client_config, server_config};
pub use verifier::PinVerifier;
