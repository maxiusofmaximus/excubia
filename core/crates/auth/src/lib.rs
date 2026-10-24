//! Pairing, request authentication and authorization for the herdr-notify
//! core: the Rust implementation of `docs/contracts/{pairing,auth,errors}.md`.
//!
//! The gateway (`gateway/`) is the behavioural reference: enrollment codes are
//! single-use and stored hashed, devices are public keys (P-256 or Ed25519),
//! every protected request carries a signature over its exact bytes, and both
//! the `401` and the `403` are uniform so the endpoints cannot be used as an
//! oracle for which devices or permissions exist.

mod authz;
mod codes;
mod curve;
mod error;
mod ratelimit;
mod store;
mod tailnet;
mod time;
mod verify;

pub use authz::{authorize, classify, Budget, EndpointRule};
pub use codes::{
    canonical_code, code_hash, mint_enrollment_code, mint_recovery_code, ENROLLMENT_TTL_SECONDS,
};
pub use curve::{verify_signature, Curve};
pub use error::{AuthzReason, EnrollmentRejected, Forbidden, StoreError, Unauthorized};
pub use ratelimit::{Class, RateLimiter, StreamRegistry};
pub use store::{
    ConflictPolicy, Device, DeviceStore, ImportEntry, ImportOutcome, Permission, PublicDevice,
    RecordError,
};
pub use tailnet::{host_tailnet_owner, is_host_owner, tailnet_owner_of};
pub use time::unix_seconds;
pub use verify::{
    body_digest, canonical_string, HeaderSource, Rejection, RequestVerifier, SignedRequest,
    VerifiedDevice, DEFAULT_CLOCK_SKEW_SECONDS, MAX_SIGNED_BODY_BYTES,
};
