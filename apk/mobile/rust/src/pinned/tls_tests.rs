//! The pinned transport against real TLS on loopback: a host identity from the
//! shared `pin` crate, served by a rustls server, read by the phone's pinned
//! client. One row per case of CORE11b.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

use crate::api::pinned::{
    pinned_request, HeaderField, PinnedEndpoint, PinnedFailureKind, PinnedReply, PinnedResponse,
};
use hn_core_pin::{server_config, unix_now, Identity, DEFAULT_VALIDITY_SECONDS};
use rustls::crypto::ring::default_provider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::version::TLS12;
use rustls::{ServerConfig, ServerConnection, StreamOwned};

const DAY: i64 = 24 * 60 * 60;
const OK_RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";

/// How the loopback host answers the one connection it accepts.
enum Host {
    Tls13(Arc<ServerConfig>),
    Tls12Only(Identity),
    Plain,
}

/// Serves exactly one connection on a fresh loopback port.
fn serve(host: Host) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    thread::spawn(move || {
        let Ok((mut tcp, _)) = listener.accept() else {
            return;
        };
        match host {
            Host::Plain => {
                let _ = tcp.write_all(OK_RESPONSE);
            }
            Host::Tls13(config) => answer(tcp, config),
            Host::Tls12Only(identity) => answer(tcp, Arc::new(tls12_config(&identity))),
        }
    });
    address
}

fn answer(tcp: TcpStream, config: Arc<ServerConfig>) {
    let Ok(connection) = ServerConnection::new(config) else {
        return;
    };
    let mut tls = StreamOwned::new(connection, tcp);
    read_head(&mut tls);
    let _ = tls.write_all(OK_RESPONSE);
    tls.conn.send_close_notify();
    let _ = tls.flush();
}

/// Reads until the request head ends, as a real server does.
fn read_head(reader: &mut impl Read) {
    let mut seen = Vec::new();
    let mut byte = [0u8; 1];
    while !seen.ends_with(b"\r\n\r\n") {
        match reader.read(&mut byte) {
            Ok(1) => seen.push(byte[0]),
            _ => return,
        }
    }
}

fn tls12_config(identity: &Identity) -> ServerConfig {
    let chain = vec![CertificateDer::from(identity.cert_der().to_vec())];
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key_der().to_vec()));
    ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_protocol_versions(&[&TLS12])
        .expect("tls 1.2 only")
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .expect("certificate")
}

fn pin_of(identity: &Identity) -> String {
    identity.fingerprint().expect("fingerprint").to_string()
}

fn endpoint(addresses: Vec<String>, primary: String, backup: Option<String>) -> PinnedEndpoint {
    PinnedEndpoint {
        addresses,
        primary,
        backup,
    }
}

fn get_health(endpoint: PinnedEndpoint) -> PinnedReply {
    let headers = vec![HeaderField {
        name: "Accept".to_string(),
        value: "application/json".to_string(),
    }];
    pinned_request(
        endpoint,
        "GET".to_string(),
        "/v1/health".to_string(),
        headers,
        Vec::new(),
    )
}

fn answered(reply: PinnedReply) -> PinnedResponse {
    match reply.response {
        Some(response) => response,
        None => panic!("expected an answer, got {:?}", reply.failure),
    }
}

fn failure_kind(reply: &PinnedReply) -> PinnedFailureKind {
    assert!(reply.response.is_none(), "the request must not be answered");
    reply.failure.as_ref().expect("a failure").kind
}

fn identity() -> Identity {
    Identity::generate().expect("identity")
}

fn host_for(identity: &Identity) -> Host {
    Host::Tls13(Arc::new(server_config(identity).expect("server config")))
}

#[test]
fn the_right_pin_is_answered_over_tls13() {
    let host = identity();
    let address = serve(host_for(&host));
    let reply = get_health(endpoint(vec![address.to_string()], pin_of(&host), None));
    let response = answered(reply);
    assert_eq!((response.status, response.body.as_str()), (200, "{}"));
    assert_eq!(response.address, address.to_string());
}

#[test]
fn a_different_certificate_with_the_old_pin_is_a_pin_mismatch() {
    let pinned = identity();
    let impostor = identity();
    let address = serve(host_for(&impostor));
    let reply = get_health(endpoint(vec![address.to_string()], pin_of(&pinned), None));
    assert_eq!(failure_kind(&reply), PinnedFailureKind::PinMismatch);
}

#[test]
fn a_rotation_accepts_the_old_and_the_new_key() {
    let old = identity();
    let new = identity();
    let old_address = serve(host_for(&old));
    let new_address = serve(host_for(&new));
    let rotating = |primary: &Identity, backup: &Identity| {
        endpoint(Vec::new(), pin_of(primary), Some(pin_of(backup)))
    };
    let on_old = get_health(PinnedEndpoint {
        addresses: vec![old_address.to_string()],
        ..rotating(&new, &old)
    });
    let on_new = get_health(PinnedEndpoint {
        addresses: vec![new_address.to_string()],
        ..rotating(&old, &new)
    });
    assert_eq!(answered(on_old).status, 200);
    assert_eq!(answered(on_new).status, 200);
}

#[test]
fn an_expired_certificate_with_its_own_pin_is_refused() {
    let expired = Identity::generate_within(unix_now() - 2 * DAY, unix_now() - DAY).expect("id");
    let address = serve(host_for(&expired));
    let reply = get_health(endpoint(vec![address.to_string()], pin_of(&expired), None));
    assert_eq!(failure_kind(&reply), PinnedFailureKind::CertificateInvalid);
}

#[test]
fn a_certificate_not_yet_valid_with_its_own_pin_is_refused() {
    let future = Identity::generate_within(
        unix_now() + DAY,
        unix_now() + DAY + DEFAULT_VALIDITY_SECONDS,
    )
    .expect("identity");
    let address = serve(host_for(&future));
    let reply = get_health(endpoint(vec![address.to_string()], pin_of(&future), None));
    assert_eq!(failure_kind(&reply), PinnedFailureKind::CertificateInvalid);
}

#[test]
fn a_tls12_only_host_is_refused_before_any_request_is_sent() {
    let host = identity();
    let pin = pin_of(&host);
    let address = serve(Host::Tls12Only(host));
    let reply = get_health(endpoint(vec![address.to_string()], pin, None));
    assert_eq!(failure_kind(&reply), PinnedFailureKind::Protocol);
}

#[test]
fn a_plain_http_host_is_refused_not_trusted() {
    let host = identity();
    let address = serve(Host::Plain);
    let reply = get_health(endpoint(vec![address.to_string()], pin_of(&host), None));
    assert_eq!(failure_kind(&reply), PinnedFailureKind::Protocol);
}

#[test]
fn an_unreachable_candidate_falls_back_to_the_next_one() {
    let host = identity();
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.local_addr().expect("address")
    };
    let address = serve(host_for(&host));
    let reply = get_health(endpoint(
        vec![closed.to_string(), address.to_string()],
        pin_of(&host),
        None,
    ));
    assert_eq!(answered(reply).address, address.to_string());
}

#[test]
fn a_pin_failure_on_a_candidate_is_final_and_not_retried_elsewhere() {
    let pinned = identity();
    let impostor = identity();
    let wrong = serve(host_for(&impostor));
    let right = serve(host_for(&pinned));
    let reply = get_health(endpoint(
        vec![wrong.to_string(), right.to_string()],
        pin_of(&pinned),
        None,
    ));
    assert_eq!(failure_kind(&reply), PinnedFailureKind::PinMismatch);
}

#[test]
fn a_malformed_pin_or_address_is_a_typed_failure() {
    let host = identity();
    let address = serve(host_for(&host));
    let bad_pin = get_health(endpoint(
        vec![address.to_string()],
        "not-a-pin".to_string(),
        None,
    ));
    assert_eq!(failure_kind(&bad_pin), PinnedFailureKind::MalformedPin);
    let bad_address = get_health(endpoint(
        vec!["host.local:8443".to_string()],
        pin_of(&host),
        None,
    ));
    assert_eq!(
        failure_kind(&bad_address),
        PinnedFailureKind::MalformedAddress
    );
}

/// The live check: a real `host` started with `--tls-bind` (see
/// `core/docs/core/evidence/core11-lan-tls.md`). The link comes from
/// `HN_LIVE_LINK`, so the one-time code never touches the repository.
#[test]
#[ignore = "needs a running host; set HN_LIVE_LINK to its pairing link"]
fn the_live_host_answers_over_its_link_pin() {
    let link = std::env::var("HN_LIVE_LINK").expect("HN_LIVE_LINK");
    let info = crate::api::pinned::parse_pairing_link(link)
        .link
        .expect("the live link parses");
    let live = |primary: String| {
        get_health(PinnedEndpoint {
            addresses: info.addresses.clone(),
            primary,
            backup: info.backup.clone(),
        })
    };
    let response = answered(live(info.primary.clone()));
    assert_eq!(response.status, 200);
    assert_eq!(response.address, info.addresses[0]);
    let refused = live(pin_of(&identity()));
    assert_eq!(failure_kind(&refused), PinnedFailureKind::PinMismatch);
}
