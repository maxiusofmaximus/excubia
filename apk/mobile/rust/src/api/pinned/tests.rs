//! Unit tests of the pairing link and the SAS entry points.

use excubia_pin::{Fingerprint, PairingLink, PinSet};

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
    // legacy: the old scheme must stay refused
    assert_eq!(
        failure_kind("herdr-notify://pair?v=1&c=AB&e=9999999999999"),
        PinnedFailureKind::MalformedLink
    );
    assert_eq!(
        failure_kind("https://example.com/pair?v=1"),
        PinnedFailureKind::MalformedLink
    );
}
