//! One pinned exchange: TCP to the first candidate that answers, a TLS 1.3
//! handshake that the pin verifier must accept, then one HTTP/1.1 request.
//! The request is written only after the handshake has proven the key.

use std::error::Error as _;
use std::io::{self, BufReader, Read};
use std::net::{SocketAddr, TcpStream};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use hn_core_pin::{client_config, Fingerprint, PinError, PinSet};
use rustls::pki_types::ServerName;
use rustls::{CertificateError, ClientConfig, ClientConnection, Error as TlsError, StreamOwned};

use super::framing::{self, Body};
use crate::api::pinned::{HeaderField, PinnedEndpoint, PinnedFailure, PinnedFailureKind as Kind};

pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(300);
pub(crate) const MAX_BODY_BYTES: u64 = 16 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) type Tls = StreamOwned<ClientConnection, TcpStream>;

/// A response whose head is read and whose body is still on the wire.
pub(crate) struct Exchange {
    pub(crate) address: SocketAddr,
    pub(crate) status: u16,
    pub(crate) retry_after: Option<u32>,
    pub(crate) body: Body<BufReader<Tls>>,
}

pub(crate) struct Request<'a> {
    pub(crate) method: &'a str,
    pub(crate) target: &'a str,
    pub(crate) headers: &'a [HeaderField],
    pub(crate) body: &'a [u8],
}

pub(crate) fn exchange(
    endpoint: &PinnedEndpoint,
    request: Request<'_>,
    io_timeout: Duration,
) -> Result<Exchange, PinnedFailure> {
    let config = Arc::new(pinned_config(endpoint)?);
    let (address, tls) = connect_first(&parse_addresses(endpoint)?, &config, io_timeout)?;
    let mut reader = BufReader::new(tls);
    framing::write_request(
        reader.get_mut(),
        request.method,
        request.target,
        &address.to_string(),
        request.headers,
        request.body,
    )
    .map_err(|error| during_exchange(&error))?;
    let head = framing::read_head(&mut reader).map_err(|error| during_exchange(&error))?;
    Ok(Exchange {
        address,
        status: head.status,
        retry_after: head.retry_after,
        body: Body::new(reader, head.framing),
    })
}

/// Reads a whole body, refusing one that exceeds the cap instead of cutting it.
pub(crate) fn read_capped(body: &mut impl Read) -> Result<Vec<u8>, PinnedFailure> {
    let mut bytes = Vec::new();
    body.take(MAX_BODY_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| during_exchange(&error))?;
    if bytes.len() as u64 > MAX_BODY_BYTES {
        return Err(PinnedFailure::of(
            Kind::Protocol,
            "the body is over the limit",
        ));
    }
    Ok(bytes)
}

fn pinned_config(endpoint: &PinnedEndpoint) -> Result<ClientConfig, PinnedFailure> {
    let primary = Fingerprint::from_str(&endpoint.primary).map_err(malformed_pin)?;
    let backup = endpoint
        .backup
        .as_deref()
        .map(Fingerprint::from_str)
        .transpose()
        .map_err(malformed_pin)?;
    client_config(PinSet::new(primary, backup))
        .map_err(|error| PinnedFailure::of(Kind::Protocol, error.to_string()))
}

fn malformed_pin(_: PinError) -> PinnedFailure {
    PinnedFailure::of(Kind::MalformedPin, "a pin is not 43 base64url characters")
}

fn parse_addresses(endpoint: &PinnedEndpoint) -> Result<Vec<SocketAddr>, PinnedFailure> {
    if endpoint.addresses.is_empty() {
        return Err(PinnedFailure::of(
            Kind::Unreachable,
            "the link has no address",
        ));
    }
    endpoint
        .addresses
        .iter()
        .map(|address| {
            SocketAddr::from_str(address).map_err(|_| {
                PinnedFailure::of(Kind::MalformedAddress, format!("{address} is not ip:port"))
            })
        })
        .collect()
}

/// Only a candidate whose TCP connect fails moves on to the next one. Anything
/// after the connect means a host answered, so a failed pin or a bad
/// certificate stops here: another address is not a second chance for a
/// different key.
fn connect_first(
    addresses: &[SocketAddr],
    config: &Arc<ClientConfig>,
    io_timeout: Duration,
) -> Result<(SocketAddr, Tls), PinnedFailure> {
    let mut last = PinnedFailure::of(Kind::Unreachable, "no candidate answered");
    for address in addresses {
        match handshake(*address, config, io_timeout) {
            Ok(tls) => return Ok((*address, tls)),
            Err(failure) if failure.kind == Kind::Unreachable => last = failure,
            Err(other) => return Err(other),
        }
    }
    Err(last)
}

fn handshake(
    address: SocketAddr,
    config: &Arc<ClientConfig>,
    io_timeout: Duration,
) -> Result<Tls, PinnedFailure> {
    let tcp = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)
        .map_err(|error| PinnedFailure::of(Kind::Unreachable, error.to_string()))?;
    tcp.set_read_timeout(Some(io_timeout))
        .and_then(|()| tcp.set_write_timeout(Some(io_timeout)))
        .map_err(|error| during_exchange(&error))?;
    let name = ServerName::IpAddress(address.ip().into());
    let connection = ClientConnection::new(Arc::clone(config), name)
        .map_err(|error| PinnedFailure::of(Kind::Protocol, error.to_string()))?;
    let mut tls = StreamOwned::new(connection, tcp);
    while tls.conn.is_handshaking() {
        tls.conn
            .complete_io(&mut tls.sock)
            .map_err(|error| during_exchange(&error))?;
    }
    Ok(tls)
}

pub(crate) fn during_exchange(error: &io::Error) -> PinnedFailure {
    if let Some(tls) = tls_error(error) {
        return classify_tls(tls);
    }
    match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => {
            PinnedFailure::of(Kind::Timeout, error.to_string())
        }
        _ => PinnedFailure::of(Kind::Protocol, error.to_string()),
    }
}

fn tls_error(error: &io::Error) -> Option<&TlsError> {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<TlsError>())
}

/// The pin verifier's own reason travels inside rustls as the source of an
/// `OtherError`, so the typed cause is recovered rather than matched on text.
fn classify_tls(error: &TlsError) -> PinnedFailure {
    match error {
        TlsError::InvalidCertificate(CertificateError::Other(other)) => {
            match other
                .source()
                .and_then(|source| source.downcast_ref::<PinError>())
            {
                Some(PinError::Unpinned) => {
                    PinnedFailure::of(Kind::PinMismatch, "the host key is not the pinned one")
                }
                Some(reason) => PinnedFailure::of(Kind::CertificateInvalid, reason.to_string()),
                None => PinnedFailure::of(Kind::Protocol, error.to_string()),
            }
        }
        TlsError::InvalidCertificate(_) => {
            PinnedFailure::of(Kind::CertificateInvalid, error.to_string())
        }
        other => PinnedFailure::of(Kind::Protocol, other.to_string()),
    }
}
