use thiserror::Error;

/// Every way the pinning rule, the host identity or the pairing link can fail.
#[derive(Debug, Error)]
pub enum PinError {
    #[error("a fingerprint is 43 unpadded base64url characters (SHA-256 of the SPKI)")]
    MalformedFingerprint,
    #[error("malformed certificate: {0}")]
    MalformedCertificate(String),
    #[error("the certificate is not valid yet")]
    NotYetValid,
    #[error("the certificate has expired")]
    Expired,
    #[error("the certificate's key is not pinned")]
    Unpinned,
    #[error("the validity window is out of range")]
    InvalidWindow,
    #[error("malformed pairing link: {0}")]
    MalformedLink(String),
    #[error("pairing link version {0} is not supported")]
    UnsupportedLinkVersion(String),
    #[error("could not create the host identity: {0}")]
    Identity(#[from] rcgen::Error),
    #[error("could not build the TLS configuration: {0}")]
    Tls(#[from] rustls::Error),
}
