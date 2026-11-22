//! The v2 pairing link, the QR without an address (F3b, `excubia_pin::PakeLink`):
//! the short code, the host's pin and when the code dies. The v1 link stays in
//! `pinned::parse_pairing_link`, which refuses a v2 one.

use excubia_pin::PakeLink;

use super::pinned::{PinnedFailure, PinnedFailureKind};

/// `host` is the pin in unpadded base64url, the form `pake_pair` takes as `expected_host`.
/// The code is a live secret: `Debug` hides it.
pub struct PakeLinkParsed {
    pub code: String,
    pub host: String,
    pub expires_at: i64,
}

impl std::fmt::Debug for PakeLinkParsed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PakeLinkParsed")
            .field("code", &"..")
            .field("host", &self.host)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Exactly one of `link` and `failure` is set. Expiry is the caller's to check
/// against `expires_at`: it needs the clock, parsing does not.
pub struct PakeLinkResult {
    pub link: Option<PakeLinkParsed>,
    pub failure: Option<PinnedFailure>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_pake_link(uri: String) -> PakeLinkResult {
    match PakeLink::parse(&uri) {
        Ok(link) => PakeLinkResult {
            link: Some(PakeLinkParsed {
                code: link.code,
                host: link.host.to_string(),
                expires_at: link.expires_at,
            }),
            failure: None,
        },
        Err(error) => PakeLinkResult {
            link: None,
            failure: Some(failure(error)),
        },
    }
}

fn failure(error: excubia_pin::PinError) -> PinnedFailure {
    use excubia_pin::PinError::{MalformedFingerprint, UnsupportedLinkVersion};
    match error {
        UnsupportedLinkVersion(version) => PinnedFailure::of(
            PinnedFailureKind::UnsupportedLinkVersion,
            format!("version {version}"),
        ),
        MalformedFingerprint => PinnedFailure::of(
            PinnedFailureKind::MalformedPin,
            "the host pin is not 43 base64url characters",
        ),
        other => PinnedFailure::of(PinnedFailureKind::MalformedLink, other.to_string()),
    }
}

#[cfg(test)]
mod tests;
