//! The pinned transport (CORE11b): the pairing link and the HTTP/SSE exchanges
//! that verify the host by its SPKI pin. Dart keeps the URL and the state; the
//! TLS rule and the link rule live here, shared with the host crate.
//!
//! Results are plain structs with an optional payload and an optional failure,
//! the same shape as the rest of this API, so FRB needs no extra codegen.

use std::io::{BufRead, BufReader};

use hn_core_pin::{unix_now, PairingLink, PinError};

use crate::frb_generated::StreamSink;
use crate::pinned::transport::{
    self, read_capped, Exchange, Request, REQUEST_TIMEOUT, STREAM_IDLE_TIMEOUT,
};

/// Where the host may be and the keys a device accepts for it. `addresses` are
/// tried in order; a pin failure on any of them is final.
pub struct PinnedEndpoint {
    pub addresses: Vec<String>,
    pub primary: String,
    pub backup: Option<String>,
}

pub struct HeaderField {
    pub name: String,
    pub value: String,
}

pub struct PairingLinkInfo {
    pub code: String,
    pub primary: String,
    pub backup: Option<String>,
    pub addresses: Vec<String>,
    pub expires_at: i64,
}

/// Exactly one of `link` and `failure` is set.
pub struct PairingLinkResult {
    pub link: Option<PairingLinkInfo>,
    pub failure: Option<PinnedFailure>,
}

pub struct PinnedResponse {
    pub status: u16,
    pub body: String,
    pub retry_after: Option<u32>,
    /// The candidate that answered, so the caller can store the one that works.
    pub address: String,
}

/// Exactly one of `response` and `failure` is set.
pub struct PinnedReply {
    pub response: Option<PinnedResponse>,
    pub failure: Option<PinnedFailure>,
}

/// Exactly one of `text` and `failure` is set; a failure is always the last item.
pub struct PinnedLine {
    pub text: Option<String>,
    pub failure: Option<PinnedFailure>,
}

/// What went wrong, as a closed set the UI can branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinnedFailureKind {
    MalformedLink,
    UnsupportedLinkVersion,
    LinkExpired,
    MalformedPin,
    MalformedAddress,
    /// The host's key is neither the pinned primary nor the backup.
    PinMismatch,
    /// The key matched but the certificate is outside its validity window.
    CertificateInvalid,
    Unreachable,
    Timeout,
    /// TLS 1.3 was not negotiated, or the peer did not speak HTTP/1.x.
    Protocol,
    /// A stream answered with something other than 200.
    Rejected,
}

/// A failure with the technical detail the UI folds away. `status` and
/// `retry_after` are only meaningful for `Rejected`; `detail` is the body then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedFailure {
    pub kind: PinnedFailureKind,
    pub detail: String,
    pub status: u16,
    pub retry_after: Option<u32>,
}

impl PinnedFailure {
    pub(crate) fn of(kind: PinnedFailureKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            status: 0,
            retry_after: None,
        }
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_pairing_link(uri: String) -> PairingLinkResult {
    parse_link(&uri, unix_now())
}

pub fn pinned_request(
    endpoint: PinnedEndpoint,
    method: String,
    target: String,
    headers: Vec<HeaderField>,
    body: Vec<u8>,
) -> PinnedReply {
    let request = Request {
        method: &method,
        target: &target,
        headers: &headers,
        body: &body,
    };
    match send(&endpoint, request) {
        Ok(response) => PinnedReply {
            response: Some(response),
            failure: None,
        },
        Err(failure) => PinnedReply {
            response: None,
            failure: Some(failure),
        },
    }
}

/// Lines of a `text/event-stream` until the host closes it.
pub fn pinned_event_lines(
    endpoint: PinnedEndpoint,
    target: String,
    headers: Vec<HeaderField>,
    sink: StreamSink<PinnedLine>,
) {
    let request = Request {
        method: "GET",
        target: &target,
        headers: &headers,
        body: &[],
    };
    let exchange = match transport::exchange(&endpoint, request, STREAM_IDLE_TIMEOUT) {
        Ok(exchange) => exchange,
        Err(failure) => {
            let _ = sink.add(stop(failure));
            return;
        }
    };
    let Exchange {
        status,
        retry_after,
        mut body,
        ..
    } = exchange;
    if status != 200 {
        let failure = match read_capped(&mut body) {
            Ok(bytes) => PinnedFailure {
                kind: PinnedFailureKind::Rejected,
                detail: String::from_utf8_lossy(&bytes).into_owned(),
                status,
                retry_after,
            },
            Err(failure) => failure,
        };
        let _ = sink.add(stop(failure));
        return;
    }
    for line in BufReader::new(body).lines() {
        let next = match line {
            Ok(text) => PinnedLine {
                text: Some(text),
                failure: None,
            },
            Err(error) => stop(transport::during_exchange(&error)),
        };
        let failed = next.failure.is_some();
        if sink.add(next).is_err() || failed {
            return;
        }
    }
}

fn stop(failure: PinnedFailure) -> PinnedLine {
    PinnedLine {
        text: None,
        failure: Some(failure),
    }
}

fn send(endpoint: &PinnedEndpoint, request: Request<'_>) -> Result<PinnedResponse, PinnedFailure> {
    let mut exchange = transport::exchange(endpoint, request, REQUEST_TIMEOUT)?;
    let bytes = read_capped(&mut exchange.body)?;
    Ok(PinnedResponse {
        status: exchange.status,
        body: String::from_utf8_lossy(&bytes).into_owned(),
        retry_after: exchange.retry_after,
        address: exchange.address.to_string(),
    })
}

fn parse_link(uri: &str, now: i64) -> PairingLinkResult {
    let link = match PairingLink::parse(uri) {
        Ok(link) => link,
        Err(error) => return refused(link_failure(error)),
    };
    if now >= link.expires_at {
        return refused(PinnedFailure::of(
            PinnedFailureKind::LinkExpired,
            format!("expired at {}", link.expires_at),
        ));
    }
    PairingLinkResult {
        link: Some(PairingLinkInfo {
            code: link.code,
            primary: link.pins.primary().to_string(),
            backup: link.pins.backup().map(|backup| backup.to_string()),
            addresses: link.addresses.iter().map(ToString::to_string).collect(),
            expires_at: link.expires_at,
        }),
        failure: None,
    }
}

fn refused(failure: PinnedFailure) -> PairingLinkResult {
    PairingLinkResult {
        link: None,
        failure: Some(failure),
    }
}

fn link_failure(error: PinError) -> PinnedFailure {
    match error {
        PinError::UnsupportedLinkVersion(version) => PinnedFailure::of(
            PinnedFailureKind::UnsupportedLinkVersion,
            format!("version {version}"),
        ),
        PinError::MalformedFingerprint => PinnedFailure::of(
            PinnedFailureKind::MalformedPin,
            "a pin is not 43 base64url characters",
        ),
        other => PinnedFailure::of(PinnedFailureKind::MalformedLink, other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use hn_core_pin::{Fingerprint, PairingLink, PinSet};

    use super::*;

    const NOW: i64 = 1_730_000_000;

    fn link_uri(expires_at: i64) -> String {
        PairingLink {
            code: "AEBAGBA".to_string(),
            pins: PinSet::new(
                Fingerprint::of_spki(b"primary"),
                Some(Fingerprint::of_spki(b"backup")),
            ),
            addresses: vec!["192.168.1.20:8443".parse().expect("address")],
            expires_at,
        }
        .to_uri()
    }

    fn failure_kind(uri: &str) -> PinnedFailureKind {
        let result = parse_link(uri, NOW);
        assert!(result.link.is_none(), "{uri} must be refused");
        result.failure.expect("a refused link has a failure").kind
    }

    #[test]
    fn a_valid_link_yields_its_pins_addresses_and_code() {
        let result = parse_link(&link_uri(NOW + 600), NOW);
        let link = result.link.expect("a valid link must parse");
        assert!(result.failure.is_none());
        assert_eq!(link.code, "AEBAGBA");
        assert_eq!(link.primary, Fingerprint::of_spki(b"primary").to_string());
        assert_eq!(
            link.backup,
            Some(Fingerprint::of_spki(b"backup").to_string())
        );
        assert_eq!(link.addresses, vec!["192.168.1.20:8443".to_string()]);
        assert_eq!(link.expires_at, NOW + 600);
    }

    #[test]
    fn an_expired_link_is_a_typed_failure() {
        assert_eq!(failure_kind(&link_uri(NOW)), PinnedFailureKind::LinkExpired);
    }

    #[test]
    fn an_unknown_link_version_is_a_typed_failure() {
        let uri = link_uri(NOW + 600).replacen("v=1", "v=9", 1);
        assert_eq!(
            failure_kind(&uri),
            PinnedFailureKind::UnsupportedLinkVersion
        );
    }

    #[test]
    fn a_malformed_pin_is_a_typed_failure() {
        let uri = link_uri(NOW + 600).replacen("h=", "h=!!", 1);
        assert_eq!(failure_kind(&uri), PinnedFailureKind::MalformedPin);
    }

    #[test]
    fn a_link_without_a_pin_or_with_junk_is_a_malformed_link() {
        assert_eq!(
            failure_kind("herdr-notify://pair?v=1&c=AB&e=9999999999999"),
            PinnedFailureKind::MalformedLink
        );
        assert_eq!(
            failure_kind("https://example.com/pair?v=1"),
            PinnedFailureKind::MalformedLink
        );
    }
}
