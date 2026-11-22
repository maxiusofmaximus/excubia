use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use excubia_pin::{client_config, PinError, PinSet};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{CertificateError, Error as TlsError, OtherError};
use tokio_rustls::TlsConnector;

use crate::{Hint, Incoming, PeerRef, Route, RouteState, Transport, TransportError, TransportKind};

/// The verifier trusts the pin alone, so the name only has to be valid.
const SERVER_NAME: &str = "excubia.lan";

/// The LAN listener's TLS 1.3 client with the host's pin. The host's routes
/// are not reached by this type yet: `connect` only checks that the host is up.
#[derive(Default)]
pub struct LanTransport {
    rtt: Mutex<Option<Duration>>,
}

impl Transport for LanTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Lan
    }

    async fn connect(&self, peer: &PeerRef) -> Result<Route, TransportError> {
        let mut refused = false;
        for &Hint::Ip(addr) in &peer.hints {
            match attempt(addr, peer).await {
                Ok(rtt) => {
                    *self.rtt.lock().expect("rtt lock") = Some(rtt);
                    return Ok(Route {
                        kind: TransportKind::Lan,
                    });
                }
                // shortcut: a hint that hangs eats the whole LAN timeout; add a per-hint timeout if real networks show it.
                Err(Miss::NoRoute) => {}
                // The PC is on but nothing listens there: try the other hints.
                Err(Miss::Refused) => refused = true,
                // A pin mismatch or refusal from one address is final, as for transports.
                Err(Miss::Stop(stop)) => return Err(stop),
            }
        }
        Err(if refused {
            TransportError::HostDown
        } else {
            TransportError::NoRoute
        })
    }

    /// shortcut: accepting is the LAN listener in crates/api (serve_client); this never resolves. Do not await it until F3.
    async fn accept(&self) -> Result<Incoming, TransportError> {
        std::future::pending().await
    }

    fn route_state(&self) -> RouteState {
        let rtt = *self.rtt.lock().expect("rtt lock");
        rtt.map_or(RouteState::Down, |rtt| RouteState::Direct { rtt })
    }
}

/// Why one address gave no route. `Refused` proves the PC is on, so it ranks
/// above `NoRoute` when no hint answers.
enum Miss {
    NoRoute,
    Refused,
    Stop(TransportError),
}

/// One address: TCP, then a TLS handshake pinned to the peer's key, then a
/// health request. Returns the health round trip.
async fn attempt(addr: SocketAddr, peer: &PeerRef) -> Result<Duration, Miss> {
    let config = client_config(PinSet::new(peer.key, None)).expect("TLS 1.3 client config");
    let connector = TlsConnector::from(Arc::new(config));
    let tcp = TcpStream::connect(addr)
        .await
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::ConnectionRefused => Miss::Refused,
            _ => Miss::NoRoute,
        })?;
    let name = ServerName::try_from(SERVER_NAME)
        .expect("constant name")
        .to_owned();
    let mut tls =
        connector
            .connect(name, tcp)
            .await
            .map_err(|error| match classify_handshake(&error) {
                TransportError::NoRoute => Miss::NoRoute,
                stop => Miss::Stop(stop),
            })?;

    // From here the host answered TLS with the pinned key: a failure is the host's.
    let request =
        format!("GET /v1/health HTTP/1.1\r\nHost: {SERVER_NAME}\r\nConnection: close\r\n\r\n");
    let started = Instant::now();
    tls.write_all(request.as_bytes())
        .await
        .map_err(|_| Miss::Stop(TransportError::HostDown))?;
    let mut response = Vec::new();
    // A peer that closes without close_notify still answered; keep what came.
    let _ = (&mut tls).take(16 * 1024).read_to_end(&mut response).await;
    let rtt = started.elapsed();

    match status_code(&response) {
        Some(200) => Ok(rtt),
        Some(401 | 403) => Err(Miss::Stop(TransportError::Refused)),
        _ => Err(Miss::Stop(TransportError::HostDown)),
    }
}

/// A handshake that fails on the pin is `IdentityMismatch`; any other failure
/// means no pinned handshake completed, which is `NoRoute`.
fn classify_handshake(error: &std::io::Error) -> TransportError {
    let unpinned = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<TlsError>())
        .is_some_and(|tls| {
            matches!(
                tls,
                TlsError::InvalidCertificate(CertificateError::Other(OtherError(cause)))
                    if matches!(cause.downcast_ref::<PinError>(), Some(PinError::Unpinned))
            )
        });
    if unpinned {
        TransportError::IdentityMismatch
    } else {
        TransportError::NoRoute
    }
}

/// The status code of an HTTP/1.x response, if the status line is there.
fn status_code(response: &[u8]) -> Option<u16> {
    std::str::from_utf8(response)
        .ok()?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use excubia_pin::{Fingerprint, Identity};
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;

    use super::*;

    /// Answers every TLS connection with `status`, as the host would. It uses
    /// the same server config as the host (`excubia_pin::server_config`).
    async fn serve(identity: &Identity, status: &'static str) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("address");
        let acceptor = TlsAcceptor::from(Arc::new(
            excubia_pin::server_config(identity).expect("server config"),
        ));
        tokio::spawn(async move {
            loop {
                let (tcp, _) = listener.accept().await.expect("accept");
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    continue;
                };
                let mut request = [0u8; 1024];
                let _ = tls.read(&mut request).await;
                let response =
                    format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                let _ = tls.write_all(response.as_bytes()).await;
                let _ = tls.shutdown().await;
            }
        });
        addr
    }

    /// An address with nothing listening: the port was free a moment ago.
    fn closed_address() -> SocketAddr {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.local_addr().expect("address")
    }

    fn peer(key: Fingerprint, hints: Vec<SocketAddr>) -> PeerRef {
        PeerRef {
            key,
            hints: hints.into_iter().map(Hint::Ip).collect(),
        }
    }

    #[tokio::test]
    async fn a_pinned_host_that_is_up_gives_a_lan_route_and_a_direct_state() {
        let identity = Identity::generate().expect("identity");
        let addr = serve(&identity, "200 OK").await;
        let transport = LanTransport::default();
        let key = identity.fingerprint().expect("fingerprint");

        let found = transport.connect(&peer(key, vec![addr])).await;
        assert_eq!(
            found,
            Ok(Route {
                kind: TransportKind::Lan
            })
        );
        assert!(matches!(transport.route_state(), RouteState::Direct { .. }));
    }

    #[tokio::test]
    async fn a_certificate_that_is_not_pinned_is_an_identity_mismatch() {
        let identity = Identity::generate().expect("identity");
        let addr = serve(&identity, "200 OK").await;
        let other = Identity::generate().expect("other identity");
        let wrong = other.fingerprint().expect("fingerprint");

        let found = LanTransport::default()
            .connect(&peer(wrong, vec![addr]))
            .await;
        assert_eq!(found, Err(TransportError::IdentityMismatch));
    }

    #[tokio::test]
    async fn an_answer_that_is_not_200_is_host_down() {
        let identity = Identity::generate().expect("identity");
        let addr = serve(&identity, "503 Service Unavailable").await;
        let key = identity.fingerprint().expect("fingerprint");

        let found = LanTransport::default()
            .connect(&peer(key, vec![addr]))
            .await;
        assert_eq!(found, Err(TransportError::HostDown));
    }

    #[tokio::test]
    async fn a_401_or_403_from_the_host_is_refused() {
        for status in ["401 Unauthorized", "403 Forbidden"] {
            let identity = Identity::generate().expect("identity");
            let addr = serve(&identity, status).await;
            let key = identity.fingerprint().expect("fingerprint");

            let found = LanTransport::default()
                .connect(&peer(key, vec![addr]))
                .await;
            assert_eq!(found, Err(TransportError::Refused), "{status}");
        }
    }

    #[tokio::test]
    async fn an_address_that_refuses_the_connection_is_host_down() {
        let key = Identity::generate()
            .expect("identity")
            .fingerprint()
            .expect("fingerprint");

        let found = LanTransport::default()
            .connect(&peer(key, vec![closed_address()]))
            .await;
        assert_eq!(found, Err(TransportError::HostDown));
    }

    #[tokio::test]
    async fn the_second_address_is_tried_when_the_first_has_no_route() {
        let identity = Identity::generate().expect("identity");
        let addr = serve(&identity, "200 OK").await;
        let key = identity.fingerprint().expect("fingerprint");

        let found = LanTransport::default()
            .connect(&peer(key, vec![closed_address(), addr]))
            .await;
        assert_eq!(
            found,
            Ok(Route {
                kind: TransportKind::Lan
            })
        );
    }

    #[tokio::test]
    async fn no_hints_is_no_route_and_no_state_is_down() {
        let key = Identity::generate()
            .expect("identity")
            .fingerprint()
            .expect("fingerprint");
        let transport = LanTransport::default();

        assert_eq!(
            transport.connect(&peer(key, vec![])).await,
            Err(TransportError::NoRoute)
        );
        assert_eq!(transport.route_state(), RouteState::Down);
    }
}
