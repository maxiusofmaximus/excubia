//! How the desktop reaches a gateway.
//!
//! A pairing link pins the host's key and the connection is TLS 1.3 with that
//! pin (`Pinned`). A typed URL reaches the host in clear (`Plain`), and only
//! where the network already protects the bytes: loopback or the tailnet.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use hn_core_pin::{client_config, PinError, PinSet};
use reqwest::Client;
use rustls::{CertificateError, OtherError};

use crate::error::{CoreError, Result};

const USER_AGENT: &str = "herdr-notify-desktop/0.1";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Plain,
    Pinned(PinSet),
}

impl Transport {
    pub fn client(&self) -> Result<Client> {
        let builder = Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(USER_AGENT);
        let builder = match self {
            Transport::Plain => builder,
            Transport::Pinned(pins) => builder.use_preconfigured_tls(
                client_config(*pins).map_err(|error| CoreError::InvalidInput(error.to_string()))?,
            ),
        };
        Ok(builder.build()?)
    }

    /// Only a pinned link may use TLS; a plain URL never does.
    pub fn accepts_scheme(&self, scheme: &str) -> bool {
        matches!(
            (self, scheme),
            (Transport::Plain, "http" | "https") | (Transport::Pinned(_), "https")
        )
    }
}

/// The base URL of a LAN address reached through a pairing link.
pub fn pinned_base_url(address: SocketAddr) -> String {
    format!("https://{address}")
}

/// Clear HTTP is for loopback, the tailnet (CGNAT range, its IPv6 prefix and
/// MagicDNS names). A LAN address is not classified here: it must come from a
/// pinned link.
pub fn plain_host_is_safe(host: &str) -> bool {
    is_loopback_host(host) || is_tailnet_host(host)
}

pub fn is_loopback_url(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(is_loopback_host))
        .unwrap_or(false)
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

fn is_tailnet_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.to_ascii_lowercase().ends_with(".ts.net") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => v4.octets()[0] == 100 && v4.octets()[1] & 0xC0 == 64,
        Ok(IpAddr::V6(v6)) => v6.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0],
        Err(_) => false,
    }
}

/// Finds the pin or certificate failure hidden in a request error. rustls
/// wraps its own error in an `io::Error`, so the source chain is walked.
pub fn tls_failure(error: &reqwest::Error) -> Option<CoreError> {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(source) = current {
        if let Some(rustls_error) = nested_rustls(source) {
            return rustls_failure(rustls_error);
        }
        current = source.source();
    }
    None
}

/// rustls errors reach the client wrapped in one or two `io::Error`s.
fn nested_rustls<'a>(error: &'a (dyn std::error::Error + 'static)) -> Option<&'a rustls::Error> {
    if let Some(rustls_error) = error.downcast_ref::<rustls::Error>() {
        return Some(rustls_error);
    }
    error
        .downcast_ref::<std::io::Error>()?
        .get_ref()
        .and_then(|inner| nested_rustls(inner))
}

fn rustls_failure(error: &rustls::Error) -> Option<CoreError> {
    let rustls::Error::InvalidCertificate(certificate) = error else {
        return None;
    };
    match certificate {
        CertificateError::Expired => Some(CoreError::CertificateExpired),
        CertificateError::NotValidYet => Some(CoreError::CertificateNotYetValid),
        CertificateError::Other(OtherError(inner)) => match inner.downcast_ref::<PinError>() {
            Some(PinError::Unpinned) => Some(CoreError::PinMismatch),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use hn_core_pin::Fingerprint;

    use super::*;

    #[test]
    fn plain_reaches_loopback_tailnet_and_magicdns_only() {
        for host in [
            "127.0.0.1",
            "::1",
            "localhost",
            "100.101.102.103",
            "[fd7a:115c:a1e0::1]",
            "mi-pc.tail1234.ts.net",
        ] {
            assert!(plain_host_is_safe(host), "{host} should be plain-safe");
        }
        for host in [
            "192.168.1.20",
            "10.0.0.5",
            "172.16.4.2",
            "mi-pc",
            "100.128.0.1",
            "example.com",
        ] {
            assert!(!plain_host_is_safe(host), "{host} must not be plain");
        }
    }

    #[test]
    fn a_pinned_transport_speaks_only_https() {
        let primary: Fingerprint = "-vJhhaleo1vBJMuBCljszQq8G6n02H6P2dSWaWP2jKI"
            .parse()
            .unwrap();
        let pins = PinSet::new(primary, None);
        assert!(Transport::Pinned(pins).accepts_scheme("https"));
        assert!(!Transport::Pinned(pins).accepts_scheme("http"));
        assert!(Transport::Plain.accepts_scheme("http"));
    }
}
