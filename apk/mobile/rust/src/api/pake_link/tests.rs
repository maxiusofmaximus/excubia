//! Unit tests of the v2 link entry point.

use std::str::FromStr;

use excubia_core_pin::{Fingerprint, PakeLink};

use super::*;
use crate::api::pinned::parse_pairing_link;

const CODE: &str = "ABCD2345";
const EXPIRES_AT: i64 = 1_730_000_180;

fn host() -> Fingerprint {
    Fingerprint::of_spki(b"the host")
}

fn uri() -> String {
    PakeLink {
        code: CODE.to_string(),
        host: host(),
        expires_at: EXPIRES_AT,
    }
    .to_uri()
}

fn refusal(uri: &str) -> PinnedFailure {
    let result = parse_pake_link(uri.to_string());
    assert!(result.link.is_none(), "{uri} must be refused");
    result.failure.expect("a refused link has a failure")
}

#[test]
fn a_generated_uri_round_trips_and_the_host_is_what_pake_pair_accepts() {
    let link = parse_pake_link(uri()).link.expect("parsed");
    assert_eq!((link.code.as_str(), link.expires_at), (CODE, EXPIRES_AT));
    assert_eq!(link.host, host().to_string());
    assert_eq!(link.host.len(), 43);
    assert_eq!(Fingerprint::from_str(&link.host).expect("pin"), host());
}

#[test]
fn an_address_or_a_backup_pin_is_refused_without_echoing_the_code() {
    for extra in ["&a=192.168.1.20:8443", "&b=AAAA"] {
        let failure = refusal(&format!("{}{extra}", uri()));
        assert_eq!(failure.kind, PinnedFailureKind::MalformedLink, "{extra}");
        assert!(!failure.detail.contains(CODE));
    }
}

#[test]
fn another_version_is_refused_and_so_is_a_v2_link_by_the_v1_entry_point() {
    let v1 = uri().replace("v=2", "v=1");
    assert_eq!(refusal(&v1).kind, PinnedFailureKind::UnsupportedLinkVersion);
    let v1_entry = parse_pairing_link(uri());
    assert!(v1_entry.link.is_none());
    assert_eq!(
        v1_entry.failure.expect("failure").kind,
        PinnedFailureKind::UnsupportedLinkVersion
    );
}

#[test]
fn a_bad_code_or_pin_is_a_typed_failure() {
    for short in [uri().replace(CODE, "ABC"), uri().replace(CODE, "ABCDEFG0")] {
        assert_eq!(refusal(&short).kind, PinnedFailureKind::MalformedLink);
    }
    let pin = host().to_string();
    let bad = uri().replace(&pin, "short");
    assert_eq!(refusal(&bad).kind, PinnedFailureKind::MalformedPin);
    // legacy: the old scheme must stay refused
    assert_eq!(
        refusal("herdr-notify://pair?v=2").kind,
        PinnedFailureKind::MalformedLink
    );
}

#[test]
fn debug_never_prints_the_code() {
    let link = parse_pake_link(uri()).link.expect("parsed");
    assert!(!format!("{link:?}").contains(CODE));
}
