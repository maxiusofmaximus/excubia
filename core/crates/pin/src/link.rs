//! The pairing link the host prints and a device reads (the QR payload).
//!
//! ```text
//! excubia://pair?v=1&c=<code>&h=<pin>[&b=<pin>]&a=<ip:port>...&e=<unix>
//! ```
//!
//! `c` is the single-use enrollment code, `h` the primary pin, `b` the backup
//! pin when a rotation is prepared, `a` each LAN candidate and `e` the code's
//! expiry. Every value is URL-safe, so no percent-encoding is needed. Unknown
//! or repeated fields are refused: a version bump is the only way to add one.
//!
//! `PakeLink` (v2, F3a) is the QR without an address: the PAKE code, the host's
//! pin and the code's expiry. The device finds the host by discovery and must
//! see that pin before it sends anything.

use std::fmt::{self, Write as _};
use std::net::SocketAddr;

use crate::{Fingerprint, PinError, PinSet};

pub const LINK_PREFIX: &str = "excubia://pair?";
pub const LINK_VERSION: u32 = 1;
const MAX_CODE_CHARS: usize = 64;
pub const PAKE_LINK_VERSION: u32 = 2;
/// Characters of a PAKE code and the symbols they come from: a copy of `hn_core_pake::Code`, which this crate
/// cannot depend on; a test compares both.
const PAKE_CODE_CHARS: usize = 8;
const PAKE_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingLink {
    pub code: String,
    pub pins: PinSet,
    pub addresses: Vec<SocketAddr>,
    pub expires_at: i64,
}

impl PairingLink {
    pub fn to_uri(&self) -> String {
        let mut uri = format!(
            "{LINK_PREFIX}v={LINK_VERSION}&c={}&h={}",
            self.code,
            self.pins.primary()
        );
        if let Some(backup) = self.pins.backup() {
            let _ = write!(uri, "&b={backup}");
        }
        for address in &self.addresses {
            let _ = write!(uri, "&a={address}");
        }
        let _ = write!(uri, "&e={}", self.expires_at);
        uri
    }

    pub fn parse(uri: &str) -> Result<Self, PinError> {
        let query = query_of(uri)?;
        let mut fields = Fields::default();
        for pair in query.split('&') {
            let (key, value) = pair
                .split_once('=')
                .ok_or_else(|| malformed("a field has no value"))?;
            match key {
                "v" => once(&mut fields.version, value)?,
                "c" => once(&mut fields.code, value)?,
                "h" => once(&mut fields.primary, value)?,
                "b" => once(&mut fields.backup, value)?,
                "e" => once(&mut fields.expires, value)?,
                "a" => fields.addresses.push(value),
                other => return Err(malformed(&format!("unknown field {other}"))),
            }
        }
        fields.finish()
    }
}

#[derive(Default)]
struct Fields<'a> {
    version: Option<&'a str>,
    code: Option<&'a str>,
    primary: Option<&'a str>,
    backup: Option<&'a str>,
    expires: Option<&'a str>,
    addresses: Vec<&'a str>,
}

impl Fields<'_> {
    fn finish(self) -> Result<PairingLink, PinError> {
        let version = required(self.version, "v")?;
        if version != LINK_VERSION.to_string() {
            return Err(PinError::UnsupportedLinkVersion(version.to_string()));
        }
        let code = required(self.code, "c")?;
        if !is_code(code) {
            return Err(malformed("the code has characters outside base32 and '-'"));
        }
        let primary: Fingerprint = required(self.primary, "h")?.parse()?;
        let backup = self.backup.map(str::parse).transpose()?;
        let addresses = self
            .addresses
            .iter()
            .map(|address| {
                address
                    .parse::<SocketAddr>()
                    .map_err(|_| malformed("an address is not ip:port"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let expires_at = required(self.expires, "e")?
            .parse::<i64>()
            .map_err(|_| malformed("the expiry is not a unix time"))?;
        Ok(PairingLink {
            code: code.to_string(),
            pins: PinSet::new(primary, backup),
            addresses,
            expires_at,
        })
    }
}

fn once<'a>(slot: &mut Option<&'a str>, value: &'a str) -> Result<(), PinError> {
    if slot.replace(value).is_some() {
        return Err(malformed("a field appears twice"));
    }
    Ok(())
}

fn required<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, PinError> {
    value.ok_or_else(|| malformed(&format!("missing field {field}")))
}

fn is_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= MAX_CODE_CHARS
        && code
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn malformed(reason: &str) -> PinError {
    PinError::MalformedLink(reason.to_string())
}

/// The query of a pairing link; any other scheme, the retired `herdr-notify://` included, is refused.
fn query_of(uri: &str) -> Result<&str, PinError> {
    uri.strip_prefix(LINK_PREFIX)
        .ok_or_else(|| malformed("not an excubia pairing link"))
}

/// The v2 link: `excubia://pair?v=2&c=<8 chars>&h=<pin>&e=<unix>`. The code is a live secret, so is the URI.
#[derive(Clone, PartialEq, Eq)]
pub struct PakeLink {
    pub code: String,
    pub host: Fingerprint,
    pub expires_at: i64,
}

impl fmt::Debug for PakeLink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PakeLink")
            .field("code", &"..")
            .field("host", &self.host)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl PakeLink {
    pub fn to_uri(&self) -> String {
        format!(
            "{LINK_PREFIX}v={PAKE_LINK_VERSION}&c={}&h={}&e={}",
            self.code, self.host, self.expires_at
        )
    }

    /// Whether `now` (unix seconds) is at or past the code's expiry. The caller asks: parsing does not.
    pub fn is_expired(&self, now: i64) -> bool {
        now >= self.expires_at
    }

    pub fn parse(uri: &str) -> Result<Self, PinError> {
        let query = query_of(uri)?;
        let (mut version, mut code, mut host, mut expires) = (None, None, None, None);
        for pair in query.split('&') {
            let (key, value) = pair
                .split_once('=')
                .ok_or_else(|| malformed("a field has no value"))?;
            match key {
                "v" => once(&mut version, value)?,
                "c" => once(&mut code, value)?,
                "h" => once(&mut host, value)?,
                "e" => once(&mut expires, value)?,
                "a" | "b" => return Err(malformed("a v2 link carries no address or backup pin")),
                other => return Err(malformed(&format!("unknown field {other}"))),
            }
        }
        let version = required(version, "v")?;
        if version != PAKE_LINK_VERSION.to_string() {
            return Err(PinError::UnsupportedLinkVersion(version.to_string()));
        }
        let code = required(code, "c")?;
        if code.len() != PAKE_CODE_CHARS || !code.bytes().all(|b| PAKE_CODE_ALPHABET.contains(&b)) {
            return Err(malformed(
                "the code is not 8 characters of the pairing alphabet",
            ));
        }
        Ok(PakeLink {
            code: code.to_string(),
            host: required(host, "h")?.parse()?,
            expires_at: required(expires, "e")?
                .parse::<i64>()
                .map_err(|_| malformed("the expiry is not a unix time"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PairingLink {
        PairingLink {
            code: "AEBAGBA".to_string(),
            pins: PinSet::new(
                Fingerprint::of_spki(b"primary"),
                Some(Fingerprint::of_spki(b"backup")),
            ),
            addresses: vec!["192.168.1.20:7443".parse().expect("address")],
            expires_at: 1_730_000_600,
        }
    }

    #[test]
    fn a_link_round_trips_and_stays_small() {
        let link = sample();
        let uri = link.to_uri();
        assert!(uri.len() < 300, "{} bytes", uri.len());
        assert_eq!(PairingLink::parse(&uri).expect("parse"), link);
    }

    #[test]
    fn the_backup_pin_and_addresses_are_optional() {
        let link = PairingLink {
            pins: PinSet::new(Fingerprint::of_spki(b"primary"), None),
            addresses: Vec::new(),
            ..sample()
        };
        assert_eq!(PairingLink::parse(&link.to_uri()).expect("parse"), link);
    }

    #[test]
    fn foreign_versions_and_fields_are_refused() {
        let uri = sample().to_uri();
        let v2 = uri.replacen("v=1", "v=2", 1);
        assert!(matches!(
            PairingLink::parse(&v2),
            Err(PinError::UnsupportedLinkVersion(_))
        ));
        let extra = format!("{uri}&x=1");
        assert!(matches!(
            PairingLink::parse(&extra),
            Err(PinError::MalformedLink(_))
        ));
        let twice = format!("{uri}&e=1");
        assert!(matches!(
            PairingLink::parse(&twice),
            Err(PinError::MalformedLink(_))
        ));
        assert!(PairingLink::parse("https://example.com/pair?v=1").is_err());
    }

    #[test]
    fn only_the_excubia_scheme_is_read() {
        let link = sample();
        let uri = link.to_uri();
        assert!(uri.starts_with("excubia://pair?v=1&"), "{uri}");
        for scheme in [
            "herdr-notify://pair?",
            "herdr://pair?",
            "excubia-x://pair?",
            "EXCUBIA://pair?",
        ] {
            let other = uri.replacen("excubia://pair?", scheme, 1);
            assert!(
                matches!(PairingLink::parse(&other), Err(PinError::MalformedLink(_))),
                "{scheme}"
            );
        }
    }

    #[test]
    fn a_malformed_pin_is_a_typed_error() {
        let uri = sample().to_uri().replacen("h=", "h=!!", 1);
        assert!(matches!(
            PairingLink::parse(&uri),
            Err(PinError::MalformedFingerprint)
        ));
    }
}
