//! The PAKE core end to end: both sides in one process, plus the negative cases of the F2a table.

use curve25519_dalek::{constants::EIGHT_TORSION, edwards::CompressedEdwardsY};
use excubia_pake::{Code, PakeError, Role, Session, SessionKey, MESSAGE_LEN};

const HOST: &str = "excubia-host-1";
const DEVICE_KEY: [u8; 65] = [4; 65];
const EXPORTER: [u8; 32] = [7; 32];
const SPKI: [u8; 32] = [9; 32];

fn code(text: &str) -> Code {
    Code::parse(text).expect("code")
}

/// Runs both sides; each gets its own code and identities so a case can break one of them.
fn run(host: (&Code, &str, &[u8]), device: (&Code, &str, &[u8])) -> (SessionKey, SessionKey) {
    let (host_session, host_msg) =
        Session::start(Role::Host, host.0, host.1, host.2).expect("host");
    let (device_session, device_msg) =
        Session::start(Role::Device, device.0, device.1, device.2).expect("device");
    (
        host_session.finish(&device_msg).expect("host finish"),
        device_session.finish(&host_msg).expect("device finish"),
    )
}

fn confirmed(
    host_key: &SessionKey,
    device_key: &SessionKey,
    host_side: [&[u8; 32]; 2],
    device_side: [&[u8; 32]; 2],
) -> bool {
    let host_tag = host_key
        .confirm_tag(Role::Host, host_side[0], host_side[1])
        .expect("tag");
    let device_tag = device_key
        .confirm_tag(Role::Device, device_side[0], device_side[1])
        .expect("tag");
    device_key
        .verify_confirm(Role::Host, device_side[0], device_side[1], &host_tag)
        .is_ok()
        && host_key
            .verify_confirm(Role::Device, host_side[0], host_side[1], &device_tag)
            .is_ok()
}

#[test]
fn same_code_and_identities_confirm_both_ways() {
    let c = code("ABCDEFGH");
    let (host_key, device_key) = run((&c, HOST, &DEVICE_KEY), (&c, HOST, &DEVICE_KEY));
    assert_eq!(host_key.expose_for_tests(), device_key.expose_for_tests());
    assert!(confirmed(
        &host_key,
        &device_key,
        [&EXPORTER, &SPKI],
        [&EXPORTER, &SPKI]
    ));
}

#[test]
fn two_runs_use_fresh_messages() {
    let c = code("ABCDEFGH");
    let (_, first) = Session::start(Role::Host, &c, HOST, &DEVICE_KEY).expect("start");
    let (_, second) = Session::start(Role::Host, &c, HOST, &DEVICE_KEY).expect("start");
    assert_ne!(first, second);
    assert_eq!(first.len(), MESSAGE_LEN);
}

#[test]
fn a_wrong_code_fails_confirmation() {
    let (host_key, device_key) = run(
        (&code("ABCDEFGH"), HOST, &DEVICE_KEY),
        (&code("ABCDEFGJ"), HOST, &DEVICE_KEY),
    );
    assert_ne!(host_key.expose_for_tests(), device_key.expose_for_tests());
    assert!(!confirmed(
        &host_key,
        &device_key,
        [&EXPORTER, &SPKI],
        [&EXPORTER, &SPKI]
    ));
}

#[test]
fn different_identities_give_different_keys() {
    let c = code("ABCDEFGH");
    let other_key = [5u8; 65];
    let cases: [(&str, &str, &[u8], &[u8]); 2] = [
        ("host id", "excubia-host-2", &DEVICE_KEY, &DEVICE_KEY),
        ("device key", HOST, &DEVICE_KEY, &other_key),
    ];
    for (name, device_host_id, host_device_key, device_device_key) in cases {
        let (host_key, device_key) = run(
            (&c, HOST, host_device_key),
            (&c, device_host_id, device_device_key),
        );
        assert_ne!(
            host_key.expose_for_tests(),
            device_key.expose_for_tests(),
            "{name}"
        );
        assert!(
            !confirmed(
                &host_key,
                &device_key,
                [&EXPORTER, &SPKI],
                [&EXPORTER, &SPKI]
            ),
            "{name}"
        );
    }
}

#[test]
fn a_relay_with_two_tls_sessions_fails_confirmation() {
    let c = code("ABCDEFGH");
    let (host_key, device_key) = run((&c, HOST, &DEVICE_KEY), (&c, HOST, &DEVICE_KEY));
    let other_exporter = [8u8; 32];
    let other_spki = [1u8; 32];
    // Same PAKE messages relayed, but each side saw its own TLS session.
    assert!(!confirmed(
        &host_key,
        &device_key,
        [&EXPORTER, &SPKI],
        [&other_exporter, &SPKI]
    ));
    // The device was shown another host certificate.
    assert!(!confirmed(
        &host_key,
        &device_key,
        [&EXPORTER, &SPKI],
        [&EXPORTER, &other_spki]
    ));
}

#[test]
fn a_tag_is_bound_to_its_role_and_length() {
    let c = code("ABCDEFGH");
    let (host_key, device_key) = run((&c, HOST, &DEVICE_KEY), (&c, HOST, &DEVICE_KEY));
    let host_tag = host_key
        .confirm_tag(Role::Host, &EXPORTER, &SPKI)
        .expect("tag");
    // Reflecting a side's own tag back does not pass as the peer's.
    assert!(host_key
        .verify_confirm(Role::Device, &EXPORTER, &SPKI, &host_tag)
        .is_err());
    assert!(device_key
        .verify_confirm(Role::Device, &EXPORTER, &SPKI, &host_tag)
        .is_err());
    assert!(device_key
        .verify_confirm(Role::Host, &EXPORTER, &SPKI, &host_tag)
        .is_ok());
    for bad in [
        &host_tag[..31],
        &host_tag[..0],
        &[host_tag.as_slice(), &[0]].concat()[..],
    ] {
        assert_eq!(
            device_key.verify_confirm(Role::Host, &EXPORTER, &SPKI, bad),
            Err(PakeError::ConfirmationFailed)
        );
    }
}

#[test]
fn messages_that_must_be_rejected() {
    let c = code("ABCDEFGH");
    let start = |role| Session::start(role, &c, HOST, &DEVICE_KEY).expect("start");
    let (_, honest_host_msg) = start(Role::Host);
    let (_, other_device_msg) = start(Role::Device);
    let torsion = EIGHT_TORSION[1].compress().to_bytes();
    let identity = {
        let mut bytes = [0u8; 32];
        bytes[0] = 1;
        bytes
    };
    let mixed = {
        let point = CompressedEdwardsY(honest_host_msg[1..].try_into().expect("32"))
            .decompress()
            .expect("point");
        (point + EIGHT_TORSION[1]).compress().to_bytes()
    };
    let with_side = |side: u8, point: &[u8]| [&[side][..], point].concat();

    let (device, own) = start(Role::Device);
    let cases: Vec<(&str, Session, Vec<u8>, PakeError)> = vec![
        (
            "empty",
            start(Role::Device).0,
            vec![],
            PakeError::MessageLength,
        ),
        (
            "32 bytes",
            start(Role::Device).0,
            honest_host_msg[..32].to_vec(),
            PakeError::MessageLength,
        ),
        (
            "34 bytes",
            start(Role::Device).0,
            [&honest_host_msg[..], &[0]].concat(),
            PakeError::MessageLength,
        ),
        ("own message reflected", device, own, PakeError::Reflected),
        (
            "same side (device to device)",
            start(Role::Device).0,
            other_device_msg,
            PakeError::BadMessage,
        ),
        (
            "unknown side byte",
            start(Role::Device).0,
            with_side(b'S', &honest_host_msg[1..]),
            PakeError::BadMessage,
        ),
        (
            "identity element",
            start(Role::Device).0,
            with_side(b'B', &identity),
            PakeError::BadMessage,
        ),
        (
            "small-order point",
            start(Role::Device).0,
            with_side(b'B', &torsion),
            PakeError::BadMessage,
        ),
        (
            "point with a torsion component",
            start(Role::Device).0,
            with_side(b'B', &mixed),
            PakeError::BadMessage,
        ),
        (
            "not a point",
            start(Role::Device).0,
            with_side(b'B', &[0xff; 32]),
            PakeError::BadMessage,
        ),
    ];
    for (name, session, message, expected) in cases {
        assert_eq!(session.finish(&message).err(), Some(expected), "{name}");
    }
}

#[test]
fn empty_identities_are_refused() {
    let c = code("ABCDEFGH");
    assert_eq!(
        Session::start(Role::Host, &c, "", &DEVICE_KEY).err(),
        Some(PakeError::EmptyIdentity)
    );
    assert_eq!(
        Session::start(Role::Host, &c, HOST, &[]).err(),
        Some(PakeError::EmptyIdentity)
    );
}

#[test]
fn debug_output_never_has_the_secrets() {
    let c = code("ABCDEFGH");
    let (host_key, _) = run((&c, HOST, &DEVICE_KEY), (&c, HOST, &DEVICE_KEY));
    let key_hex = data_encoding::HEXLOWER.encode(&host_key.expose_for_tests());
    let (session, _) = Session::start(Role::Host, &c, HOST, &DEVICE_KEY).expect("start");
    for text in [
        format!("{c:?}"),
        format!("{host_key:?}"),
        format!("{session:?}"),
    ] {
        assert!(
            !text.contains("ABCDEFGH") && !text.contains(&key_hex),
            "{text}"
        );
    }
}

/// Documents why `check_point` exists: the pinned `spake2` accepts the identity element on its own.
#[test]
fn the_library_alone_accepts_the_identity_element() {
    use spake2::{Ed25519Group, Identity, Password, Spake2};
    let (spake, _) = Spake2::<Ed25519Group>::start_a(
        &Password::new(b"ABCDEFGH"),
        &Identity::new(b"a"),
        &Identity::new(b"b"),
    );
    let mut identity = [0u8; 33];
    identity[0] = b'B';
    identity[1] = 1;
    assert!(spake.finish(&identity).is_ok());
}
