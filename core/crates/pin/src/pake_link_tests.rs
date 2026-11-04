//! Tests of the v2 link (`PakeLink`); the v1 ones live next to `PairingLink`.

use crate::{Fingerprint, PairingLink, PakeLink, PinError};

fn sample() -> PakeLink {
    PakeLink {
        code: "ABCD2345".to_string(),
        host: Fingerprint::of_spki(b"host"),
        expires_at: 1_730_000_180,
    }
}

fn refused(uri: &str) -> PinError {
    PakeLink::parse(uri).expect_err(uri)
}

#[test]
fn a_v2_link_round_trips_in_the_documented_shape() {
    let link = sample();
    let uri = link.to_uri();
    assert_eq!(
        uri,
        format!("excubia://pair?v=2&c=ABCD2345&h={}&e=1730000180", link.host)
    );
    assert!(uri.len() < 120, "{} bytes", uri.len());
    assert_eq!(PakeLink::parse(&uri).expect("parse"), link);
}

#[test]
fn the_old_scheme_is_read_and_the_new_one_is_written() {
    let link = sample();
    let legacy = link.to_uri().replacen("excubia://", "herdr-notify://", 1);
    assert_eq!(PakeLink::parse(&legacy).expect("legacy parse"), link);
    assert!(link.to_uri().starts_with("excubia://pair?v=2&"));
}

#[test]
fn an_unknown_scheme_is_refused_like_any_other_foreign_link() {
    let uri = sample().to_uri();
    for scheme in ["herdr://pair?", "excubia-x://pair?", "https://pair?"] {
        let other = uri.replacen("excubia://pair?", scheme, 1);
        assert!(
            matches!(refused(&other), PinError::MalformedLink(_)),
            "{scheme}"
        );
    }
}

#[test]
fn an_address_or_a_backup_pin_is_refused() {
    let uri = sample().to_uri();
    for extra in ["a=192.168.1.20:7443", "b=AAAA"] {
        assert!(matches!(
            refused(&format!("{uri}&{extra}")),
            PinError::MalformedLink(_)
        ));
    }
}

#[test]
fn other_versions_and_fields_are_refused() {
    let uri = sample().to_uri();
    for version in ["v=1", "v=3", "v="] {
        let other = uri.replacen("v=2", version, 1);
        assert!(matches!(
            refused(&other),
            PinError::UnsupportedLinkVersion(_)
        ));
    }
    for extra in ["x=1", "e=1", "c=ABCD2345", "h=AAAA"] {
        assert!(
            matches!(
                refused(&format!("{uri}&{extra}")),
                PinError::MalformedLink(_)
            ),
            "{extra}"
        );
    }
    for missing in ["v=2&", "&e=1730000180"] {
        assert!(PakeLink::parse(&uri.replacen(missing, "", 1)).is_err());
    }
    assert!(PakeLink::parse("https://example.com/pair?v=2").is_err());
    // The v1 parser does not read a v2 link, and the other way round.
    assert!(matches!(
        PairingLink::parse(&uri),
        Err(PinError::UnsupportedLinkVersion(_))
    ));
}

#[test]
fn a_bad_code_pin_or_expiry_is_a_typed_error() {
    let uri = sample().to_uri();
    for code in [
        "ABCD234",
        "ABCD23456",
        "ABCD234I",
        "ABCD2340",
        "abcd2345",
        "ABCD-234",
        "ABCD234\u{e9}",
        "",
    ] {
        let other = uri.replacen("ABCD2345", code, 1);
        assert!(
            matches!(refused(&other), PinError::MalformedLink(_)),
            "{code}"
        );
    }
    let pin = sample().host.to_string();
    for bad in ["!!", "AAAA", &format!("{pin}A")] {
        assert!(matches!(
            refused(&uri.replacen(&pin, bad, 1)),
            PinError::MalformedFingerprint
        ));
    }
    for bad in ["soon", "", "1.5"] {
        let other = uri.replacen("e=1730000180", &format!("e={bad}"), 1);
        assert!(
            matches!(refused(&other), PinError::MalformedLink(_)),
            "{bad}"
        );
    }
}

#[test]
fn the_code_alphabet_is_the_one_that_mints_codes() {
    for symbol in (0u8..128)
        .map(char::from)
        .filter(|c| !c.is_ascii_lowercase() && *c != '-')
    {
        let code = format!("AAAAAAA{symbol}");
        let minted = hn_core_pake::Code::parse(&code).is_ok();
        let uri = sample().to_uri().replacen("ABCD2345", &code, 1);
        assert_eq!(PakeLink::parse(&uri).is_ok(), minted, "{symbol:?}");
    }
    for _ in 0..50 {
        let code = hn_core_pake::Code::generate().expect("rng");
        let link = PakeLink {
            code: code.reveal().to_owned(),
            ..sample()
        };
        assert_eq!(PakeLink::parse(&link.to_uri()).expect("parse"), link);
    }
}

#[test]
fn debug_does_not_print_the_code() {
    let shown = format!("{:?}", sample());
    assert!(!shown.contains("ABCD2345"), "{shown}");
}

#[test]
fn a_link_expires_at_its_expiry_and_parsing_does_not_check() {
    let link = sample();
    assert!(!link.is_expired(link.expires_at - 1));
    assert!(link.is_expired(link.expires_at));
    assert!(link.is_expired(link.expires_at + 1));
    let old = PakeLink {
        expires_at: 1,
        ..link
    };
    assert_eq!(PakeLink::parse(&old.to_uri()).expect("parse"), old);
}
