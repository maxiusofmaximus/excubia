//! The pairing link a host prints (`herdr-notify://pair?...`). The desktop
//! checks it before it dials: a known version, a pin of the right shape and an
//! unexpired code. The pin then decides which host answers.

use std::net::SocketAddr;

use hn_core_pin::{unix_now, PairingLink, PinError, PinSet};

use crate::client::{pair_device, PairResponse};
use crate::error::{CoreError, Result};
use crate::transport::{pinned_base_url, Transport};

/// A link that is current and well formed. Its code is still unspent: the
/// host decides whether it is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairLink {
    pub code: String,
    pub pins: PinSet,
    pub addresses: Vec<SocketAddr>,
    pub expires_at: i64,
}

/// The host that answered the link, and the base URL it answered at.
#[derive(Debug)]
pub struct LinkPairing {
    pub response: PairResponse,
    pub base_url: String,
}

impl PairLink {
    pub fn parse_now(uri: &str) -> Result<Self> {
        Self::parse(uri, unix_now())
    }

    pub fn parse(uri: &str, now: i64) -> Result<Self> {
        let link = PairingLink::parse(uri.trim()).map_err(invalid)?;
        if now >= link.expires_at {
            return Err(CoreError::LinkExpired);
        }
        if link.addresses.is_empty() {
            return Err(CoreError::LinkInvalid(
                "el enlace no indica ninguna dirección".to_string(),
            ));
        }
        Ok(Self {
            code: link.code,
            pins: link.pins,
            addresses: link.addresses,
            expires_at: link.expires_at,
        })
    }
}

/// Pairs through the first address whose certificate matches the pin. A
/// different host on that address (or an expired certificate) only moves on
/// to the next one; an answer from a pinned host is final.
pub async fn pair_through_link(
    link: &PairLink,
    public_key: &str,
    label: &str,
) -> Result<LinkPairing> {
    let mut failure = None;
    for address in &link.addresses {
        let base_url = pinned_base_url(*address);
        let transport = Transport::Pinned(link.pins);
        match pair_device(&base_url, &transport, "p256", public_key, label, &link.code).await {
            Ok(response) => return Ok(LinkPairing { response, base_url }),
            Err(error) if is_next_candidate(&error) => {
                failure.get_or_insert(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(failure.unwrap_or_else(|| {
        CoreError::LinkInvalid("el enlace no indica ninguna dirección".to_string())
    }))
}

fn is_next_candidate(error: &CoreError) -> bool {
    matches!(
        error,
        CoreError::Transport(_)
            | CoreError::PinMismatch
            | CoreError::CertificateExpired
            | CoreError::CertificateNotYetValid
    )
}

fn invalid(error: PinError) -> CoreError {
    CoreError::LinkInvalid(match error {
        PinError::UnsupportedLinkVersion(version) => {
            format!("la versión {version} no es compatible; actualiza Herdr Notify")
        }
        PinError::MalformedFingerprint => "la huella del host no es válida".to_string(),
        _ => "faltan datos o el formato no es válido".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK: &str = "herdr-notify://pair?v=1&c=ABCD-2345&h=-vJhhaleo1vBJMuBCljszQq8G6n02H6P2dSWaWP2jKI&a=192.168.1.20:8443&e=2000";

    #[test]
    fn a_current_link_parses_into_its_pins_and_addresses() {
        let link = PairLink::parse(LINK, 1000).expect("parses");
        assert_eq!(link.code, "ABCD-2345");
        assert_eq!(link.addresses, vec!["192.168.1.20:8443".parse().unwrap()]);
        assert!(link.pins.backup().is_none());
    }

    #[test]
    fn an_expired_link_is_refused_with_its_own_error() {
        assert!(matches!(
            PairLink::parse(LINK, 2000),
            Err(CoreError::LinkExpired)
        ));
    }

    #[test]
    fn an_unknown_version_names_the_version() {
        let uri = LINK.replace("v=1", "v=9");
        let message = PairLink::parse(&uri, 1000).unwrap_err().to_string();
        assert!(message.contains("versión 9"), "{message}");
    }

    #[test]
    fn a_malformed_pin_and_a_link_without_addresses_are_typed_errors() {
        let bad_pin = LINK.replace("h=-vJhhaleo1vBJMuBCljszQq8G6n02H6P2dSWaWP2jKI", "h=abc");
        assert!(
            matches!(PairLink::parse(&bad_pin, 1000), Err(CoreError::LinkInvalid(message)) if message.contains("huella"))
        );
        let no_address = LINK.replace("&a=192.168.1.20:8443", "");
        assert!(matches!(
            PairLink::parse(&no_address, 1000),
            Err(CoreError::LinkInvalid(_))
        ));
    }
}
