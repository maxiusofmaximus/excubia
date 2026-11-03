//! `pake_pair` against the fake host of `pake_fake` on real TLS loopback.

use std::net::TcpListener;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hn_core_pin::Identity;
use serde_json::Value;

use super::pake_fake::{serve, Fake, Quirk, HOST_CODE};
use crate::api::pinned::{pake_pair, PakeFailureKind as Kind, PakePairResult};
use crate::api::signing::generate_device_key;

const WRONG_CODE: &str = "ZZZZ9999";

fn identity() -> Identity {
    Identity::generate().expect("identity")
}

fn pair(addresses: Vec<String>, code: &str) -> PakePairResult {
    pake_pair(
        addresses,
        code.to_string(),
        generate_device_key().public_key_b64,
    )
}

fn failure_kind(result: &PakePairResult) -> Kind {
    assert!(result.outcome.is_none(), "the pairing must fail");
    result.failure.as_ref().expect("a failure").kind
}

fn paths(fake: &Fake) -> Vec<String> {
    fake.seen()
        .connections
        .iter()
        .map(|raw| {
            let text = String::from_utf8_lossy(raw).into_owned();
            text.split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

/// The same host answering two connections, as in a real pairing.
fn honest(quirk: Quirk) -> Fake {
    let host = identity();
    serve(vec![host.clone(), host], quirk)
}

fn address(fake: &Fake) -> Vec<String> {
    vec![fake.address.to_string()]
}

fn no_connection_was_made(listener: &TcpListener) -> bool {
    listener.set_nonblocking(true).expect("nonblocking");
    matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
}

#[test]
fn the_right_code_pairs_and_returns_the_spki_that_the_pake_confirmed() {
    let fake = honest(Quirk::Honest);
    let result = pair(address(&fake), HOST_CODE);
    let outcome = result.outcome.expect("paired");
    assert_eq!(
        (outcome.device_id.as_str(), outcome.recovery_code.as_str()),
        ("dev-test-1", "RC-TEST")
    );
    assert_eq!(outcome.pin, fake.pin);
    assert_eq!(outcome.address, fake.address.to_string());
    assert_eq!(
        paths(&fake),
        ["/v1/pair/pake/start", "/v1/pair/pake/finish"]
    );
    assert_eq!(fake.seen().device_tag_ok, Some(true));
}

#[test]
fn each_connection_has_its_own_exporter_and_the_tag_is_bound_to_the_second() {
    let fake = honest(Quirk::Honest);
    assert!(pair(address(&fake), HOST_CODE).outcome.is_some());
    let seen = fake.seen();
    assert_eq!(seen.exporters.len(), 2);
    assert_ne!(seen.exporters[0], seen.exporters[1]);
    assert_eq!(seen.tag_fits_first_exporter, Some(false));
}

#[test]
fn a_wrong_code_is_caught_by_the_hosts_tag_before_anything_else_is_sent() {
    let fake = honest(Quirk::Honest);
    let result = pair(address(&fake), WRONG_CODE);
    assert_eq!(failure_kind(&result), Kind::HostProofInvalid);
    assert_eq!(
        paths(&fake),
        ["/v1/pair/pake/start"],
        "no finish may be sent"
    );
}

#[test]
fn a_forged_host_tag_is_refused_even_with_the_right_code() {
    let fake = honest(Quirk::FakeHostTag);
    let result = pair(address(&fake), HOST_CODE);
    assert_eq!(failure_kind(&result), Kind::HostProofInvalid);
    assert_eq!(paths(&fake), ["/v1/pair/pake/start"]);
}

#[test]
fn a_relay_with_another_certificate_on_the_second_connection_fails_unpinned() {
    let fake = serve(vec![identity(), identity()], Quirk::Honest);
    let result = pair(address(&fake), HOST_CODE);
    assert_eq!(failure_kind(&result), Kind::ChannelChanged);
    assert_eq!(
        paths(&fake),
        ["/v1/pair/pake/start"],
        "the tag must not reach the relay"
    );
}

#[test]
fn the_hosts_refusals_at_start_have_their_own_kinds() {
    let rows = [
        (410, "no_live_code", Kind::CodeGone),
        (403, "pake_requires_tls", Kind::NoPake),
        (404, "not_found", Kind::NoPake),
        (400, "invalid_body", Kind::Protocol),
        (400, "pake_invalid_message", Kind::Protocol),
        (429, "slow down", Kind::RateLimited),
        (500, "internal", Kind::Protocol),
    ];
    for (status, detail, expected) in rows {
        let fake = honest(Quirk::RefuseStart(status, detail));
        let result = pair(address(&fake), HOST_CODE);
        assert_eq!(failure_kind(&result), expected, "{status} {detail}");
        if status == 429 {
            assert_eq!(result.failure.expect("failure").retry_after, Some(7));
        }
    }
}

#[test]
fn the_hosts_refusals_at_finish_have_their_own_kinds() {
    let rows = [
        (403, "confirmation_failed", Kind::HostProofInvalid),
        (410, "pake_session_gone", Kind::CodeGone),
        (429, "slow down", Kind::RateLimited),
        (400, "invalid_body", Kind::Protocol),
    ];
    for (status, detail, expected) in rows {
        let fake = honest(Quirk::RefuseFinish(status, detail));
        let result = pair(address(&fake), HOST_CODE);
        assert_eq!(failure_kind(&result), expected, "{status} {detail}");
        assert_eq!(
            paths(&fake).len(),
            2,
            "the refusal came from the finish step"
        );
    }
}

#[test]
fn a_badly_formed_code_never_touches_the_network() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let target = vec![listener.local_addr().expect("address").to_string()];
    for text in [
        "",
        "ABC",
        "ABCDEFG0",
        "ABCDEFGO",
        "ABCD-EFGH-J",
        "ABCDEFG\u{e9}",
    ] {
        let result = pair(target.clone(), text);
        assert_eq!(failure_kind(&result), Kind::InvalidCodeFormat, "{text:?}");
    }
    assert!(no_connection_was_made(&listener));
}

#[test]
fn a_bad_device_key_never_touches_the_network() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let target = vec![listener.local_addr().expect("address").to_string()];
    for key in [
        "not base64!".to_string(),
        STANDARD.encode([4u8; 33]),
        STANDARD.encode([4u8; 65]),
    ] {
        let result = pake_pair(target.clone(), HOST_CODE.to_string(), key);
        assert_eq!(failure_kind(&result), Kind::Protocol);
    }
    assert!(no_connection_was_made(&listener));
}

#[test]
fn the_first_connection_carries_only_the_spake_message_and_no_code_travels() {
    let fake = honest(Quirk::Honest);
    assert!(pair(address(&fake), HOST_CODE).outcome.is_some());
    let seen = fake.seen();
    let first = String::from_utf8_lossy(&seen.connections[0]).into_owned();
    let json = first.split("\r\n\r\n").nth(1).expect("body");
    let body: Value = serde_json::from_str(json).expect("json");
    let mut fields: Vec<&str> = body
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(fields, ["device_pubkey", "message"]);
    for raw in &seen.connections {
        let text = String::from_utf8_lossy(raw).to_uppercase();
        assert!(
            !text.contains(HOST_CODE),
            "the code travelled in clear text"
        );
    }
}

#[test]
fn an_unreachable_candidate_falls_back_but_a_protocol_failure_does_not() {
    let closed = TcpListener::bind("127.0.0.1:0").expect("bind");
    let closed_address = closed.local_addr().expect("address").to_string();
    drop(closed);

    let host = honest(Quirk::Honest);
    let both = vec![closed_address.clone(), host.address.to_string()];
    let paired = pair(both, HOST_CODE).outcome.expect("paired");
    assert_eq!(paired.address, host.address.to_string());

    let burned = honest(Quirk::RefuseStart(410, "no_live_code"));
    let spare = honest(Quirk::Honest);
    let both = vec![burned.address.to_string(), spare.address.to_string()];
    assert_eq!(failure_kind(&pair(both, HOST_CODE)), Kind::CodeGone);
    assert!(
        spare.seen().connections.is_empty(),
        "the next address must not be tried"
    );

    let result = pair(vec![closed_address], HOST_CODE);
    assert_eq!(failure_kind(&result), Kind::Unreachable);
}
