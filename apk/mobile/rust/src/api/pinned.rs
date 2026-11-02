//! The pinned transport (CORE11b): the pairing link and the HTTP/SSE exchanges
//! that verify the host by its SPKI pin. Dart keeps the URL and the state; the
//! TLS rule and the link rule live here, shared with the host crate.
//!
//! Results are plain structs with an optional payload and an optional failure,
//! the same shape as the rest of this API, so FRB needs no extra codegen.

use std::io::{BufRead, BufReader};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hn_core_pin::{sas, unix_now, Fingerprint, PairingLink, PinError};

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

/// A binary answer, kept as the bytes the host sent.
pub struct PinnedBytesResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub retry_after: Option<u32>,
    pub address: String,
}

/// Exactly one of `response` and `failure` is set.
pub struct PinnedBytesReply {
    pub response: Option<PinnedBytesResponse>,
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
    /// The PC refused the connection: it is on, but nothing listens on the port.
    Refused,
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

/// The six-digit string both screens show after a pinned pairing (RFC
/// pairing-security §5, F1). `None` when the pin or the key is malformed.
#[flutter_rust_bridge::frb(sync)]
pub fn pairing_sas(pin: String, public_key_base64: String) -> Option<String> {
    let pin: Fingerprint = pin.parse().ok()?;
    let key = STANDARD.decode(public_key_base64).ok()?;
    Some(sas(&pin, &key))
}

pub fn pinned_request(
    endpoint: PinnedEndpoint,
    method: String,
    target: String,
    headers: Vec<HeaderField>,
    body: Vec<u8>,
) -> PinnedReply {
    let reply = pinned_request_bytes(endpoint, method, target, headers, body);
    PinnedReply {
        response: reply.response.map(text_response),
        failure: reply.failure,
    }
}

/// Like `pinned_request`, but the body is returned as the bytes the host sent.
pub fn pinned_request_bytes(
    endpoint: PinnedEndpoint,
    method: String,
    target: String,
    headers: Vec<HeaderField>,
    body: Vec<u8>,
) -> PinnedBytesReply {
    let request = Request {
        method: &method,
        target: &target,
        headers: &headers,
        body: &body,
    };
    match send_bytes(&endpoint, request) {
        Ok(response) => PinnedBytesReply {
            response: Some(response),
            failure: None,
        },
        Err(failure) => PinnedBytesReply {
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

fn text_response(raw: PinnedBytesResponse) -> PinnedResponse {
    PinnedResponse {
        status: raw.status,
        body: String::from_utf8_lossy(&raw.body).into_owned(),
        retry_after: raw.retry_after,
        address: raw.address,
    }
}

fn send_bytes(
    endpoint: &PinnedEndpoint,
    request: Request<'_>,
) -> Result<PinnedBytesResponse, PinnedFailure> {
    let mut exchange = transport::exchange(endpoint, request, REQUEST_TIMEOUT)?;
    let body = read_capped(&mut exchange.body)?;
    Ok(PinnedBytesResponse {
        status: exchange.status,
        body,
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
    fn pairing_sas_matches_the_contract_vectors() {
        let pin = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8".to_string();
        let mut key_a = vec![0x04_u8];
        key_a.extend([0x11_u8; 32]);
        key_a.extend([0x22_u8; 32]);
        let mut key_b = vec![0x04_u8];
        key_b.extend([0x33_u8; 32]);
        key_b.extend([0x44_u8; 32]);
        assert_eq!(
            pairing_sas(pin.clone(), STANDARD.encode(key_a)),
            Some("097665".to_string())
        );
        assert_eq!(
            pairing_sas(pin, STANDARD.encode(key_b)),
            Some("827242".to_string())
        );
    }

    #[test]
    fn pairing_sas_refuses_a_malformed_pin_or_key() {
        let key = STANDARD.encode([0x04_u8; 65]);
        assert_eq!(pairing_sas("short".to_string(), key), None);
        assert_eq!(pairing_sas("A".repeat(43), "not base64!".to_string()), None);
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
