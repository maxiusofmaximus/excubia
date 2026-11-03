//! The fake host of the `pake_pair` tests: a rustls server with a `hn_core_pin::Identity` that speaks the
//! host side of `/v1/pair/pake/*` with `hn_core_pake::Role::Host` and reads its own RFC 9266 exporter.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hn_core_pake::{Code, Role, Session, SessionKey};
use hn_core_pin::{server_config, Identity};
use rustls::{ServerConnection, StreamOwned};
use serde_json::{json, Value};

pub(super) const HOST_CODE: &str = "ABCD2345";

#[derive(Clone, Copy)]
pub(super) enum Quirk {
    Honest,
    FakeHostTag,
    RefuseStart(u16, &'static str),
    RefuseFinish(u16, &'static str),
}

#[derive(Default)]
pub(super) struct Seen {
    /// Decrypted request bytes of each connection, in accept order.
    pub(super) connections: Vec<Vec<u8>>,
    pub(super) exporters: Vec<[u8; 32]>,
    pub(super) device_tag_ok: Option<bool>,
    pub(super) tag_fits_first_exporter: Option<bool>,
}

pub(super) struct Fake {
    pub(super) address: SocketAddr,
    pub(super) seen: Arc<Mutex<Seen>>,
    pub(super) pin: String,
}

impl Fake {
    pub(super) fn seen(&self) -> std::sync::MutexGuard<'_, Seen> {
        self.seen.lock().expect("seen")
    }
}

/// One connection per identity, in order; the first identity is the host's.
pub(super) fn serve(identities: Vec<Identity>, quirk: Quirk) -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    let pin = identities[0].fingerprint().expect("fingerprint");
    let seen = Arc::new(Mutex::new(Seen::default()));
    let record = Arc::clone(&seen);
    thread::spawn(move || {
        let host_id = hex::encode(pin.as_bytes());
        let mut key: Option<SessionKey> = None;
        for identity in identities {
            let Ok((tcp, _)) = listener.accept() else {
                return;
            };
            let config = Arc::new(server_config(&identity).expect("server config"));
            let Ok(connection) = ServerConnection::new(config) else {
                return;
            };
            let mut tls = StreamOwned::new(connection, tcp);
            let Some((path, body, raw)) = read_request(&mut tls) else {
                continue;
            };
            let spki = *identity.fingerprint().expect("spki").as_bytes();
            let exporter = tls
                .conn
                .export_keying_material([0u8; 32], b"EXPORTER-Channel-Binding", None)
                .expect("exporter");
            let mut seen = record.lock().expect("seen");
            seen.connections.push(raw);
            seen.exporters.push(exporter);
            let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let (status, answer) = if path.ends_with("/start") {
                match quirk {
                    Quirk::RefuseStart(status, detail) => (status, json!({ "detail": detail })),
                    _ => start(&body, &host_id, &exporter, &spki, quirk, &mut key),
                }
            } else {
                match quirk {
                    Quirk::RefuseFinish(status, detail) => (status, json!({ "detail": detail })),
                    _ => finish(&body, &exporter, &spki, key.as_ref(), &mut seen),
                }
            };
            drop(seen);
            respond(&mut tls, status, &answer);
        }
    });
    Fake {
        address,
        seen,
        pin: pin.to_string(),
    }
}

fn start(
    body: &Value,
    host_id: &str,
    exporter: &[u8; 32],
    spki: &[u8; 32],
    quirk: Quirk,
    key: &mut Option<SessionKey>,
) -> (u16, Value) {
    let field = |name: &str| STANDARD.decode(body[name].as_str().unwrap_or_default());
    let (Ok(device), Ok(message)) = (field("device_pubkey"), field("message")) else {
        return (400, json!({ "detail": "invalid_body" }));
    };
    let code = Code::parse(HOST_CODE).expect("code");
    let (session, own) = Session::start(Role::Host, &code, host_id, &device).expect("host");
    let Ok(shared) = session.finish(&message) else {
        return (400, json!({ "detail": "pake_invalid_message" }));
    };
    let tag = match quirk {
        Quirk::FakeHostTag => [9u8; 32],
        _ => shared.confirm_tag(Role::Host, exporter, spki).expect("tag"),
    };
    *key = Some(shared);
    (
        200,
        json!({ "message": STANDARD.encode(own), "tag": STANDARD.encode(tag) }),
    )
}

fn finish(
    body: &Value,
    exporter: &[u8; 32],
    spki: &[u8; 32],
    key: Option<&SessionKey>,
    seen: &mut Seen,
) -> (u16, Value) {
    let tag = STANDARD
        .decode(body["tag"].as_str().unwrap_or_default())
        .unwrap_or_default();
    let Some(key) = key else {
        return (410, json!({ "detail": "pake_session_gone" }));
    };
    let ok = key
        .verify_confirm(Role::Device, exporter, spki, &tag)
        .is_ok();
    seen.device_tag_ok = Some(ok);
    seen.tag_fits_first_exporter = seen
        .exporters
        .first()
        .map(|first| key.verify_confirm(Role::Device, first, spki, &tag).is_ok());
    if !ok {
        return (403, json!({ "detail": "confirmation_failed" }));
    }
    (
        200,
        json!({ "ok": true, "device_id": "dev-test-1", "permissions": [], "recovery_code": "RC-TEST" }),
    )
}

fn read_request(tls: &mut impl Read) -> Option<(String, Vec<u8>, Vec<u8>)> {
    let mut raw = Vec::new();
    let mut byte = [0u8; 1];
    while !raw.ends_with(b"\r\n\r\n") {
        match tls.read(&mut byte) {
            Ok(1) => raw.push(byte[0]),
            _ => return None,
        }
    }
    let head = String::from_utf8_lossy(&raw).into_owned();
    let length = head
        .to_lowercase()
        .lines()
        .find_map(|line| line.strip_prefix("content-length:")?.trim().parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    tls.read_exact(&mut body).ok()?;
    raw.extend_from_slice(&body);
    let path = head.split_whitespace().nth(1)?.to_owned();
    Some((path, body, raw))
}

fn respond(
    tls: &mut StreamOwned<ServerConnection, std::net::TcpStream>,
    status: u16,
    body: &Value,
) {
    let body = body.to_string();
    let retry = if status == 429 {
        "Retry-After: 7\r\n"
    } else {
        ""
    };
    let head = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{retry}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = tls.write_all(head.as_bytes());
    let _ = tls.write_all(body.as_bytes());
    tls.conn.send_close_notify();
    let _ = tls.flush();
}
