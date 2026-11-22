//! The LAN listener: TLS 1.3 on an explicit address, handing axum only
//! connections whose handshake completed. Plaintext never reaches the router,
//! and the router it feeds is the device surface alone (`serve_client`).
//!
//! Every connection holds a slot from its accept until it closes, under two
//! caps (all peers together, and one source address), so a peer that opens
//! sockets and parks them cannot exhaust the host. `serve_connections` runs
//! hyper directly rather than `axum::serve`, because `axum::serve` does not
//! expose the header read timer.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::extract::connect_info::Connected;
use axum::extract::ConnectInfo;
use axum::serve::{IncomingStream, Listener};
use axum::{Extension, Router};
use hyper::server::conn::http1::Builder;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use rustls::ServerConfig;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

use crate::PeerAddr;

/// A client that opens a socket and stalls the handshake is dropped after this.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Completed handshakes waiting for axum to take them.
const READY_BACKLOG: usize = 64;
const ACCEPT_RETRY: Duration = Duration::from_secs(1);

/// Connections the LAN listener holds at once, all peers together.
const MAX_CONNECTIONS: usize = 256;
/// Connections one source address may hold. Loopback is exempt (the desktop
/// and the hooks run on this machine) and counts only against the total.
const MAX_CONNECTIONS_PER_IP: usize = 16;
// shortcut: hyper arms this one timer for the header read and for the wait
// between requests, so an idle keep-alive connection also closes at 10 s, not
// the 60 s asked for; a separate idle timer needs a request-aware reader of our
// own. Upgrade if a client drops its idle connections.
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug)]
struct ConnectionLimits {
    total: usize,
    per_ip: usize,
    header_timeout: Duration,
}

const LAN_LIMITS: ConnectionLimits = ConnectionLimits {
    total: MAX_CONNECTIONS,
    per_ip: MAX_CONNECTIONS_PER_IP,
    header_timeout: HEADER_READ_TIMEOUT,
};

type Ready = (Admitted, SocketAddr);
type PerAddress = Arc<Mutex<HashMap<IpAddr, usize>>>;

/// Hands out connection slots under the two caps.
struct ConnectionGate {
    limits: ConnectionLimits,
    total: Arc<Semaphore>,
    per_address: PerAddress,
}

impl ConnectionGate {
    fn new(limits: ConnectionLimits) -> Self {
        Self {
            limits,
            total: Arc::new(Semaphore::new(limits.total)),
            per_address: Arc::default(),
        }
    }

    /// The slot for a connection from `peer`, or `None` when a cap is full.
    /// The caller drops the socket then, with no answer.
    fn admit(&self, peer: IpAddr) -> Option<ConnectionGuard> {
        let permit = Arc::clone(&self.total).try_acquire_owned().ok()?;
        if peer.is_loopback() {
            return Some(ConnectionGuard {
                _permit: permit,
                address: None,
            });
        }
        let mut counts = lock(&self.per_address);
        let held = counts.get(&peer).copied().unwrap_or(0);
        if held >= self.limits.per_ip {
            return None;
        }
        counts.insert(peer, held + 1);
        Some(ConnectionGuard {
            _permit: permit,
            address: Some((Arc::clone(&self.per_address), peer)),
        })
    }
}

/// The slots of one connection. Dropping it, when the connection closes or
/// its handshake fails, gives them back.
struct ConnectionGuard {
    _permit: OwnedSemaphorePermit,
    address: Option<(PerAddress, IpAddr)>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let Some((counts, peer)) = self.address.take() else {
            return;
        };
        let mut counts = lock(&counts);
        match counts.get(&peer).copied() {
            Some(held) if held > 1 => {
                counts.insert(peer, held - 1);
            }
            _ => {
                counts.remove(&peer);
            }
        }
    }
}

fn lock(counts: &Mutex<HashMap<IpAddr, usize>>) -> MutexGuard<'_, HashMap<IpAddr, usize>> {
    counts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// An accepted TLS connection and the slots it holds. The stream is all that
/// axum and hyper see; the slots leave with the connection.
pub struct Admitted {
    tls: TlsStream<TcpStream>,
    _guard: ConnectionGuard,
}

impl AsyncRead for Admitted {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().tls).poll_read(cx, buf)
    }
}

impl AsyncWrite for Admitted {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().tls).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().tls).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.tls.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().tls).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().tls).poll_shutdown(cx)
    }
}

/// The LAN listener. Accepting hands out only connections that completed their
/// handshake, each with its slots attached.
pub struct TlsListener {
    local: SocketAddr,
    ready: mpsc::Receiver<Ready>,
    limits: ConnectionLimits,
}

impl TlsListener {
    pub async fn bind(address: SocketAddr, config: Arc<ServerConfig>) -> io::Result<Self> {
        Self::bind_with(address, config, LAN_LIMITS).await
    }

    async fn bind_with(
        address: SocketAddr,
        config: Arc<ServerConfig>,
        limits: ConnectionLimits,
    ) -> io::Result<Self> {
        let tcp = TcpListener::bind(address).await?;
        let local = tcp.local_addr()?;
        let (sender, ready) = mpsc::channel(READY_BACKLOG);
        let gate = Arc::new(ConnectionGate::new(limits));
        tokio::spawn(accept_loop(tcp, TlsAcceptor::from(config), sender, gate));
        Ok(Self {
            local,
            ready,
            limits,
        })
    }
}

impl Listener for TlsListener {
    type Io = Admitted;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.ready.recv().await {
            Some(connection) => connection,
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(self.local)
    }
}

impl Connected<IncomingStream<'_, TlsListener>> for PeerAddr {
    fn connect_info(stream: IncomingStream<'_, TlsListener>) -> Self {
        Self {
            addr: *stream.remote_addr(),
            binding: exporter(stream.io().tls.get_ref().1),
        }
    }
}

/// The label RFC 9266 registers for `tls-exporter`; the context is empty.
const EXPORTER_LABEL: &[u8] = b"EXPORTER-Channel-Binding";

/// The channel binding of an established TLS 1.3 connection, or `None` if rustls will not give one.
pub(crate) fn exporter(connection: &rustls::ServerConnection) -> Option<[u8; 32]> {
    let mut binding = [0u8; 32];
    connection
        .export_keying_material(&mut binding, EXPORTER_LABEL, None)
        .ok()?;
    Some(binding)
}

async fn accept_loop(
    tcp: TcpListener,
    acceptor: TlsAcceptor,
    ready: mpsc::Sender<Ready>,
    gate: Arc<ConnectionGate>,
) {
    loop {
        tokio::select! {
            _ = ready.closed() => return,
            accepted = tcp.accept() => match accepted {
                Ok((stream, peer)) => {
                    // Over a cap the socket is dropped here, unanswered.
                    if let Some(guard) = gate.admit(peer.ip()) {
                        tokio::spawn(handshake(acceptor.clone(), stream, peer, ready.clone(), guard));
                    }
                }
                Err(_) => tokio::time::sleep(ACCEPT_RETRY).await,
            },
        }
    }
}

/// Each handshake runs on its own task, so one slow peer cannot hold up the
/// accept loop for the others. A failed or stalled handshake is dropped
/// silently: the peer gets no answer and no detail, which is also what a
/// plaintext client should see. Its slot goes with it.
async fn handshake(
    acceptor: TlsAcceptor,
    stream: TcpStream,
    peer: SocketAddr,
    ready: mpsc::Sender<Ready>,
    guard: ConnectionGuard,
) {
    if let Ok(Ok(tls)) = tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await {
        let _ = ready.send((Admitted { tls, _guard: guard }, peer)).await;
    }
}

/// Serves `app` on the LAN listener over HTTP/1.1, with the header read timer
/// set. The server advertises no ALPN, so no client negotiates HTTP/2 here and
/// the HTTP/2 detection of `axum::serve` is not kept. Each connection's peer
/// address and channel binding are its `ConnectInfo<PeerAddr>`.
pub(crate) async fn serve_connections(mut listener: TlsListener, app: Router) -> io::Result<()> {
    let header_timeout = listener.limits.header_timeout;
    loop {
        let (connection, peer) = listener.accept().await;
        let binding = exporter(connection.tls.get_ref().1);
        let service = app.clone().layer(Extension(ConnectInfo(PeerAddr {
            addr: peer,
            binding,
        })));
        tokio::spawn(async move {
            let mut builder = Builder::new();
            builder
                .timer(TokioTimer::new())
                .header_read_timeout(header_timeout);
            let _ = builder
                .serve_connection(TokioIo::new(connection), TowerToHyperService::new(service))
                .await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::extract::ConnectInfo;
    use axum::routing::get;
    use excubia_pin::{client_config, server_config, Identity, PinSet};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_rustls::client;
    use tokio_rustls::rustls::pki_types::ServerName;
    use tokio_rustls::TlsConnector;

    const ANSWER: &str = "pong 127.0.0.1";

    /// Answers with the peer address the connection reports, so the test sees `ConnectInfo` too.
    fn app() -> Router {
        Router::new().route(
            "/ping",
            get(|ConnectInfo(peer): ConnectInfo<PeerAddr>| async move {
                format!("pong {}", peer.addr.ip())
            }),
        )
    }

    /// A LAN listener under `limits`, serving `app()`. Returns its address and the pins a client trusts.
    async fn serving(limits: ConnectionLimits) -> (SocketAddr, PinSet) {
        let identity = Identity::generate().expect("identity");
        let pins = PinSet::new(identity.fingerprint().expect("fingerprint"), None);
        let config = Arc::new(server_config(&identity).expect("server config"));
        let listener =
            TlsListener::bind_with("127.0.0.1:0".parse().expect("address"), config, limits)
                .await
                .expect("bind");
        let addr = listener.local_addr().expect("local address");
        tokio::spawn(serve_connections(listener, app()));
        (addr, pins)
    }

    async fn connect_tls(
        addr: SocketAddr,
        pins: &PinSet,
    ) -> io::Result<client::TlsStream<TcpStream>> {
        let config = client_config(*pins).expect("client config");
        let tcp = TcpStream::connect(addr).await?;
        let name = ServerName::try_from("excubia.lan")
            .expect("name")
            .to_owned();
        TlsConnector::from(Arc::new(config))
            .connect(name, tcp)
            .await
    }

    /// A client connecting until the server has room again: released slots are freed asynchronously.
    async fn connect_when_free(addr: SocketAddr, pins: &PinSet) -> client::TlsStream<TcpStream> {
        for _ in 0..100 {
            if let Ok(tls) = connect_tls(addr, pins).await {
                return tls;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no slot was freed in time");
    }

    /// One request on `tls`; returns the answer once it has arrived whole.
    async fn ping(tls: &mut client::TlsStream<TcpStream>) -> String {
        tls.write_all(b"GET /ping HTTP/1.1\r\nHost: lan\r\n\r\n")
            .await
            .expect("write");
        let mut seen = Vec::new();
        let read = async {
            let mut chunk = [0u8; 256];
            while !String::from_utf8_lossy(&seen).contains(ANSWER) {
                let n = tls.read(&mut chunk).await.expect("read");
                assert!(n > 0, "the server closed before answering");
                seen.extend_from_slice(&chunk[..n]);
            }
        };
        tokio::time::timeout(Duration::from_secs(5), read)
            .await
            .expect("answer in time");
        String::from_utf8_lossy(&seen).into_owned()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn past_the_total_cap_a_socket_is_closed_unanswered_and_parked_ones_free_their_slots() {
        let (addr, pins) = serving(ConnectionLimits {
            total: 2,
            ..LAN_LIMITS
        })
        .await;
        let parked_a = TcpStream::connect(addr).await.expect("connect");
        let parked_b = TcpStream::connect(addr).await.expect("connect");

        // Both slots are held by sockets that never speak: a third is closed at once.
        let mut third = TcpStream::connect(addr).await.expect("connect");
        let mut byte = [0u8; 1];
        let closed = tokio::time::timeout(Duration::from_secs(5), third.read(&mut byte))
            .await
            .expect("closed in time");
        assert!(matches!(closed, Ok(0) | Err(_)), "{closed:?}");

        // Closing the parked sockets frees the slots, and a real client gets through.
        drop(parked_a);
        drop(parked_b);
        let mut tls = connect_when_free(addr, &pins).await;
        assert!(ping(&mut tls).await.contains(ANSWER));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_slot_is_held_until_its_connection_closes_not_only_through_the_handshake() {
        let (addr, pins) = serving(ConnectionLimits {
            total: 1,
            ..LAN_LIMITS
        })
        .await;
        let mut first = connect_tls(addr, &pins).await.expect("first");
        assert!(ping(&mut first).await.contains(ANSWER));

        // The first connection is open and idle: its slot is the only one.
        assert!(
            connect_tls(addr, &pins).await.is_err(),
            "a second must wait"
        );

        drop(first);
        let mut again = connect_when_free(addr, &pins).await;
        assert!(ping(&mut again).await.contains(ANSWER));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_connection_that_stops_mid_header_is_closed_at_the_header_timeout() {
        let (addr, pins) = serving(ConnectionLimits {
            header_timeout: Duration::from_millis(300),
            ..LAN_LIMITS
        })
        .await;
        let mut tls = connect_tls(addr, &pins).await.expect("handshake");
        tls.write_all(b"GET /ping HTTP/1.1\r\nHost: lan\r\n")
            .await
            .expect("half a header");

        let mut rest = Vec::new();
        let closed = tokio::time::timeout(Duration::from_secs(5), tls.read_to_end(&mut rest)).await;
        assert!(closed.is_ok(), "the server must close a stalled header");
        assert!(!String::from_utf8_lossy(&rest).contains("pong"));
    }

    #[test]
    fn the_address_cap_counts_each_address_apart_and_exempts_loopback() {
        let gate = ConnectionGate::new(ConnectionLimits {
            total: 100,
            per_ip: 2,
            ..LAN_LIMITS
        });
        let peer: IpAddr = "192.0.2.10".parse().expect("address");
        let other: IpAddr = "192.0.2.11".parse().expect("address");
        let first = gate.admit(peer).expect("first from the address");
        let _second = gate.admit(peer).expect("second from the address");
        assert!(gate.admit(peer).is_none(), "a third from one address");
        assert!(gate.admit(other).is_some(), "another address is apart");

        let loopback: IpAddr = "127.0.0.1".parse().expect("address");
        let held: Vec<_> = (0..5).filter_map(|_| gate.admit(loopback)).collect();
        assert_eq!(held.len(), 5, "loopback is not held to the address cap");

        drop(first);
        assert!(gate.admit(peer).is_some(), "a dropped slot is given back");
    }
}
