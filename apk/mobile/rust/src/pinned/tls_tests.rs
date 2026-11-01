//! The pinned transport against real TLS on loopback: a host identity from the
//! shared `pin` crate, served by a rustls server, read by the phone's pinned
//! client. One row per case of CORE11b.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

use crate::api::pinned::{
    pinned_request, pinned_request_bytes, HeaderField, PinnedBytesReply, PinnedBytesResponse,
    PinnedEndpoint, PinnedFailureKind, PinnedReply, PinnedResponse,
};
use hn_core_pin::{server_config, unix_now, Identity, DEFAULT_VALIDITY_SECONDS};
use rustls::crypto::ring::default_provider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::version::TLS12;
use rustls::{ServerConfig, ServerConnection, StreamOwned};

use super::transport::MAX_BODY_BYTES;

const DAY: i64 = 24 * 60 * 60;
const OK_RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";

/// How the loopback host answers the one connection it accepts.
enum Host {
    Tls13(Arc<ServerConfig>),
    Tls13Reply(Arc<ServerConfig>, Vec<u8>),
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
            Host::Tls13(config) => answer(tcp, config, OK_RESPONSE),
            Host::Tls13Reply(config, reply) => answer(tcp, config, &reply),
            Host::Tls12Only(identity) => {
                answer(tcp, Arc::new(tls12_config(&identity)), OK_RESPONSE)
            }
        }
    });
    address
}

fn answer(tcp: TcpStream, config: Arc<ServerConfig>, reply: &[u8]) {
    let Ok(connection) = ServerConnection::new(config) else {
        return;
    };
    let mut tls = StreamOwned::new(connection, tcp);
    read_head(&mut tls);
    let _ = tls.write_all(reply);
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

/// The whole response a host sends for one WebP body.
fn webp_reply(body: &[u8]) -> Vec<u8> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: image/webp\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    [head.as_bytes(), body].concat()
}

fn get_bytes(endpoint: PinnedEndpoint, target: &str) -> PinnedBytesReply {
    pinned_request_bytes(
        endpoint,
        "GET".to_string(),
        target.to_string(),
        Vec::new(),
        Vec::new(),
    )
}

fn answered_bytes(reply: PinnedBytesReply) -> PinnedBytesResponse {
    match reply.response {
        Some(response) => response,
        None => panic!("expected an answer, got {:?}", reply.failure),
    }
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
fn a_binary_body_arrives_byte_for_byte() {
    let host = identity();
    // Not UTF-8: a text decode would replace the 0xFF and 0x80 bytes.
    let mut image = b"RIFF\x10\x00\x00\x00WEBPVP8 ".to_vec();
    image.extend_from_slice(&[0xFF, 0x80, 0x00, 0xC3]);
    let config = Arc::new(server_config(&host).expect("server config"));
    let address = serve(Host::Tls13Reply(config, webp_reply(&image)));
    let reply = get_bytes(
        endpoint(vec![address.to_string()], pin_of(&host), None),
        "/v1/chat/conversations/c1/attachments/a1/content",
    );
    let response = answered_bytes(reply);
    assert_eq!((response.status, response.body), (200, image));
}

#[test]
fn a_binary_body_over_the_cap_is_refused_with_the_cap_error() {
    let host = identity();
    let oversized = vec![0u8; MAX_BODY_BYTES as usize + 1];
    let config = Arc::new(server_config(&host).expect("server config"));
    let address = serve(Host::Tls13Reply(config, webp_reply(&oversized)));
    let reply = get_bytes(
        endpoint(vec![address.to_string()], pin_of(&host), None),
        "/v1/chat/conversations/c1/attachments/a1/content",
    );
    assert!(reply.response.is_none(), "the oversized body must not pass");
    let failure = reply.failure.expect("a failure");
    assert_eq!(
        (failure.kind, failure.detail.as_str()),
        (PinnedFailureKind::Protocol, "the body is over the limit")
    );
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
