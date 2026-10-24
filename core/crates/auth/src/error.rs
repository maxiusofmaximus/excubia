//! Typed failures. The public HTTP shapes live in `hn-core-api`; these are the
//! library-level reasons, kept separate so the caller can log the specific one
//! while answering uniformly.

use thiserror::Error;

use crate::store::Permission;
use crate::verify::Rejection;

/// The trust store could not be read, parsed or written.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("trust store io: {0}")]
    Io(#[from] std::io::Error),
    #[error("trust store is malformed: {0}")]
    Malformed(#[from] serde_json::Error),
    #[error("unsupported trust store version {0}")]
    Version(u32),
    #[error("no data directory available for the trust store")]
    NoDataDir,
}

/// A signed request was refused. The caller answers a uniform `401`.
#[derive(Debug, Error)]
#[error("unauthorized ({reason:?})")]
pub struct Unauthorized {
    pub reason: Rejection,
}

impl Unauthorized {
    pub fn new(reason: Rejection) -> Self {
        Self { reason }
    }
}

/// An authenticated request was refused. The caller answers a uniform `403`.
#[derive(Debug, Error)]
#[error("forbidden ({reason:?}, required {required:?})")]
pub struct Forbidden {
    pub reason: AuthzReason,
    pub required: Option<Permission>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthzReason {
    NotPermitted,
    UnknownEndpoint,
}

/// An enrollment code was unknown, expired, already used, or the request
/// could not register the device it named. The caller answers a uniform
/// `403 pairing_rejected` when a code was supplied.
#[derive(Debug, Error)]
#[error("enrollment rejected: {0}")]
pub struct EnrollmentRejected(pub String);

impl EnrollmentRejected {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
