//! The LAN listener: TLS 1.3 on an explicit address, handing axum only
//! connections whose handshake completed. Plaintext never reaches the router,
//! and the router it feeds is the device surface alone (`serve_client`).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::connect_info::Connected;
use axum::serve::{IncomingStream, Listener};
use rustls::ServerConfig;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;

use crate::PeerAddr;

/// A client that opens a socket and stalls the handshake is dropped after this.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Completed handshakes waiting for axum to take them.
const READY_BACKLOG: usize = 64;
const ACCEPT_RETRY: Duration = Duration::from_secs(1);

type Ready = (TlsStream<TcpStream>, SocketAddr);

/// Each handshake runs on its own task, so one slow peer cannot hold up the
/// accept loop for the others.
pub struct TlsListener {
    local: SocketAddr,
    ready: mpsc::Receiver<Ready>,
}

impl TlsListener {
    pub async fn bind(address: SocketAddr, config: Arc<ServerConfig>) -> std::io::Result<Self> {
        let tcp = TcpListener::bind(address).await?;
        let local = tcp.local_addr()?;
        let (sender, ready) = mpsc::channel(READY_BACKLOG);
        tokio::spawn(accept_loop(tcp, TlsAcceptor::from(config), sender));
        Ok(Self { local, ready })
    }
}

impl Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.ready.recv().await {
            Some(connection) => connection,
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(self.local)
    }
}

impl Connected<IncomingStream<'_, TlsListener>> for PeerAddr {
    fn connect_info(stream: IncomingStream<'_, TlsListener>) -> Self {
        Self(*stream.remote_addr())
    }
}

async fn accept_loop(tcp: TcpListener, acceptor: TlsAcceptor, ready: mpsc::Sender<Ready>) {
    loop {
        tokio::select! {
            _ = ready.closed() => return,
            accepted = tcp.accept() => match accepted {
                Ok((stream, peer)) => {
                    tokio::spawn(handshake(acceptor.clone(), stream, peer, ready.clone()));
                }
                Err(_) => tokio::time::sleep(ACCEPT_RETRY).await,
            },
        }
    }
}

/// A failed or stalled handshake is dropped silently: the peer gets no answer
/// and no detail, which is also what a plaintext client should see.
async fn handshake(
    acceptor: TlsAcceptor,
    stream: TcpStream,
    peer: SocketAddr,
    ready: mpsc::Sender<Ready>,
) {
    if let Ok(Ok(tls)) = tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await {
        let _ = ready.send((tls, peer)).await;
    }
}
