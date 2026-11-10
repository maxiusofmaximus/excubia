//! Interchangeable transports to a paired host, identified by key and never by
//! IP (`docs/rfc/transport.md` §4, phase F1). LAN is the only implementation.

mod lan;

use std::future::Future;
use std::net::SocketAddr;
use std::time::Duration;

use excubia_core_pin::Fingerprint;
use thiserror::Error;

pub use lan::LanTransport;

/// The identity a peer is pinned to: the SHA-256 of the host's SPKI, which is
/// what the pairing link carries. It is a fingerprint of the key, not the key.
pub type PublicKey = Fingerprint;

/// Declaration order is the attempt order of RFC §4; `Tunnel` is last resort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TransportKind {
    Lan,
    P2p,
    Vpn,
    Tunnel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    Ip(SocketAddr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRef {
    pub key: PublicKey,
    pub hints: Vec<Hint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteState {
    Direct { rtt: Duration },
    Relayed { rtt: Duration },
    Down,
}

/// A working path to the peer. Minimal in F1: which transport won.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    pub kind: TransportKind,
}

/// An accepted inbound path. Minimal in F1: which transport it came through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Incoming {
    pub kind: TransportKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TransportError {
    #[error("No llego al PC por ninguna ruta. ¿Está encendido y en la misma red? Si usas la VPN, compruébala.")]
    NoRoute,
    #[error(
        "El PC responde, pero el servicio está parado. Arráncalo en el PC y vuelve a intentarlo."
    )]
    HostDown,
    #[error("El PC ya no es el que emparejaste. Vuelve a emparejarlo desde el PC.")]
    IdentityMismatch,
    #[error("Este móvil ya no tiene acceso a ese PC. Pide un nuevo emparejamiento.")]
    Refused,
}

pub trait Transport {
    fn kind(&self) -> TransportKind;
    fn connect(&self, peer: &PeerRef)
        -> impl Future<Output = Result<Route, TransportError>> + Send;
    fn accept(&self) -> impl Future<Output = Result<Incoming, TransportError>> + Send;
    fn route_state(&self) -> RouteState;
}

/// Tries the transports in RFC §4 order (whatever order `transports` has), each
/// with at most `per_transport`. The first route wins. `IdentityMismatch` and
/// `Refused` stop the search at once. Otherwise the most specific error wins:
/// `HostDown` if some transport reached the host, else `NoRoute`.
pub async fn connect_first<T: Transport>(
    transports: &[T],
    peer: &PeerRef,
    per_transport: Duration,
) -> Result<Route, TransportError> {
    let mut order: Vec<&T> = transports.iter().collect();
    order.sort_by_key(|transport| transport.kind());
    let mut reached_host = false;
    for transport in order {
        let outcome = tokio::time::timeout(per_transport, transport.connect(peer))
            .await
            .unwrap_or(Err(TransportError::NoRoute));
        match outcome {
            Ok(route) => return Ok(route),
            Err(TransportError::HostDown) => reached_host = true,
            Err(TransportError::NoRoute) => {}
            Err(stop) => return Err(stop),
        }
    }
    Err(if reached_host {
        TransportError::HostDown
    } else {
        TransportError::NoRoute
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;

    /// `outcome: None` never answers, to exercise the per-transport timeout.
    struct Fake {
        kind: TransportKind,
        outcome: Option<Result<Route, TransportError>>,
        calls: Arc<AtomicUsize>,
    }

    impl Transport for Fake {
        fn kind(&self) -> TransportKind {
            self.kind
        }

        async fn connect(&self, _peer: &PeerRef) -> Result<Route, TransportError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.outcome {
                Some(outcome) => outcome,
                None => std::future::pending().await,
            }
        }

        async fn accept(&self) -> Result<Incoming, TransportError> {
            std::future::pending().await
        }

        fn route_state(&self) -> RouteState {
            RouteState::Down
        }
    }

    fn fake(
        kind: TransportKind,
        outcome: Result<Route, TransportError>,
    ) -> (Fake, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let transport = Fake {
            kind,
            outcome: Some(outcome),
            calls: Arc::clone(&calls),
        };
        (transport, calls)
    }

    fn route(kind: TransportKind) -> Result<Route, TransportError> {
        Ok(Route { kind })
    }

    fn peer() -> PeerRef {
        PeerRef {
            key: Fingerprint::of_spki(b"host"),
            hints: Vec::new(),
        }
    }

    const PLAZO: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn the_first_route_wins_and_the_next_transport_is_not_called() {
        let (lan, lan_calls) = fake(TransportKind::Lan, route(TransportKind::Lan));
        let (p2p, p2p_calls) = fake(TransportKind::P2p, route(TransportKind::P2p));
        let found = connect_first(&[lan, p2p], &peer(), PLAZO).await;
        assert_eq!(found, route(TransportKind::Lan));
        assert_eq!(lan_calls.load(Ordering::SeqCst), 1);
        assert_eq!(p2p_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn no_route_moves_on_to_the_next_transport() {
        let (lan, lan_calls) = fake(TransportKind::Lan, Err(TransportError::NoRoute));
        let (p2p, p2p_calls) = fake(TransportKind::P2p, route(TransportKind::P2p));
        let found = connect_first(&[lan, p2p], &peer(), PLAZO).await;
        assert_eq!(found, route(TransportKind::P2p));
        assert_eq!(lan_calls.load(Ordering::SeqCst), 1);
        assert_eq!(p2p_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn attempts_follow_the_rfc_order_whatever_the_input_order() {
        let (vpn, vpn_calls) = fake(TransportKind::Vpn, route(TransportKind::Vpn));
        let (p2p, _) = fake(TransportKind::P2p, route(TransportKind::P2p));
        let (lan, _) = fake(TransportKind::Lan, Err(TransportError::NoRoute));
        let found = connect_first(&[vpn, p2p, lan], &peer(), PLAZO).await;
        assert_eq!(found, route(TransportKind::P2p));
        assert_eq!(vpn_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_transport_that_never_answers_times_out_into_no_route() {
        let calls = Arc::new(AtomicUsize::new(0));
        let hanging = Fake {
            kind: TransportKind::Lan,
            outcome: None,
            calls: Arc::clone(&calls),
        };
        let (p2p, _) = fake(TransportKind::P2p, route(TransportKind::P2p));
        let found = connect_first(&[hanging, p2p], &peer(), Duration::from_millis(50)).await;
        assert_eq!(found, route(TransportKind::P2p));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn identity_mismatch_and_refusal_never_try_the_next_transport() {
        for stop in [TransportError::IdentityMismatch, TransportError::Refused] {
            let (lan, _) = fake(TransportKind::Lan, Err(stop));
            let (p2p, p2p_calls) = fake(TransportKind::P2p, route(TransportKind::P2p));
            let found = connect_first(&[lan, p2p], &peer(), PLAZO).await;
            assert_eq!(found, Err(stop));
            assert_eq!(
                p2p_calls.load(Ordering::SeqCst),
                0,
                "{stop:?} must not retry"
            );
        }
    }

    #[tokio::test]
    async fn the_most_specific_error_wins_when_all_fail() {
        let cases = [
            (
                TransportError::HostDown,
                TransportError::NoRoute,
                TransportError::HostDown,
            ),
            (
                TransportError::NoRoute,
                TransportError::HostDown,
                TransportError::HostDown,
            ),
            (
                TransportError::NoRoute,
                TransportError::NoRoute,
                TransportError::NoRoute,
            ),
        ];
        for (lan_outcome, p2p_outcome, expected) in cases {
            let (lan, _) = fake(TransportKind::Lan, Err(lan_outcome));
            let (p2p, _) = fake(TransportKind::P2p, Err(p2p_outcome));
            let found = connect_first(&[lan, p2p], &peer(), PLAZO).await;
            assert_eq!(found, Err(expected));
        }
    }

    #[tokio::test]
    async fn no_transports_means_no_route() {
        let none: [Fake; 0] = [];
        assert_eq!(
            connect_first(&none, &peer(), PLAZO).await,
            Err(TransportError::NoRoute)
        );
    }

    #[test]
    fn messages_are_the_spanish_ones_from_the_rfc() {
        assert_eq!(
            TransportError::NoRoute.to_string(),
            "No llego al PC por ninguna ruta. ¿Está encendido y en la misma red? Si usas la VPN, compruébala."
        );
        assert_eq!(
            TransportError::HostDown.to_string(),
            "El PC responde, pero el servicio está parado. Arráncalo en el PC y vuelve a intentarlo."
        );
        assert_eq!(
            TransportError::IdentityMismatch.to_string(),
            "El PC ya no es el que emparejaste. Vuelve a emparejarlo desde el PC."
        );
        assert_eq!(
            TransportError::Refused.to_string(),
            "Este móvil ya no tiene acceso a ese PC. Pide un nuevo emparejamiento."
        );
    }
}
