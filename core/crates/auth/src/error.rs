//! Typed failures. The public HTTP shapes live in `hn-core-api`; these are the
//! library-level reasons, kept separate so the caller can log the specific one
//! while answering uniformly.

use hn_core_persist::RandomError;
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
    #[error(transparent)]
    Random(#[from] RandomError),
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
/// `403 pairing_rejected` when a code was supplied. A failure of the host
/// itself (no entropy) is `internal`: the caller answers `500`, not a refusal
/// the client would blame on the code.
#[derive(Debug, Error)]
#[error("enrollment rejected: {message}")]
pub struct EnrollmentRejected {
    pub message: String,
    pub internal: bool,
}

impl EnrollmentRejected {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            internal: false,
        }
    }
}

impl From<RandomError> for EnrollmentRejected {
    fn from(error: RandomError) -> Self {
        Self {
            message: error.to_string(),
            internal: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_random_failure_is_internal_and_a_bad_code_is_not() {
        assert!(EnrollmentRejected::from(RandomError).internal);
        assert!(!EnrollmentRejected::new("unknown").internal);
    }
}
