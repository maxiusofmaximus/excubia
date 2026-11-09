//! Shared rig for the PAKE pairing tests: a real host over a real TLS listener and the test device.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::harness::{raw_exchange, TestHost};
use axum::serve::Listener;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use hn_core_api::{
    serve, serve_client, AgentActions, AlfredBackend, ApiConfig, ApiContext, PakeSlot, TlsListener,
};
use hn_core_host::pairing_audit::AUDIT_FILE;
use hn_core_host::pairing_notice::PairingNotice;
use hn_core_host::tls_slots::TlsSlots;
use hn_core_pake::{Code, Role, Session, SessionKey};
use hn_core_pin::{client_config, CertificateInfo, PinSet};
use hn_core_test_support::DemoIdentity;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::TlsConnector;

pub const START: &str = "/v1/pair/pake/start";
pub const FINISH: &str = "/v1/pair/pake/finish";
pub const ADMIN: &str = "/v1/admin/pake-code";

/// A host with its device routes on TLS (`lan`) and its loopback router on plaintext (`plain`), both over the
/// same context, whose PAKE slot reads a clock the test moves.
pub struct Rig {
    pub host: TestHost,
    pub lan: SocketAddr,
    pub plain: SocketAddr,
    pub pins: PinSet,
    pub ctx: ApiContext,
    pub clock: Arc<Mutex<Duration>>,
    /// What the device computes as `host_id` from the certificate it sees.
    pub host_id: String,
}

impl Rig {
    pub async fn new(name: &str) -> Rig {
        let host = TestHost::start(name).await;
        let slots = TlsSlots::open(&host.data_dir);
        let identity = slots.active().expect("identity");
        let pin = slots.active_pin().expect("pin");
        let config = Arc::new(hn_core_pin::server_config(&identity).expect("server config"));
        let listener = TlsListener::bind("127.0.0.1:0".parse().expect("address"), config)
            .await
            .expect("bind");
        let lan = listener.local_addr().expect("local address");
        let actions: Arc<dyn AgentActions> = Arc::clone(&host.host) as Arc<dyn AgentActions>;
        let mut ctx = ApiContext::new(ApiConfig {
            store: Arc::clone(&host.store),
            actions,
            devices: Arc::clone(&host.devices),
            agents: Arc::new(hn_core_host::load_agents(&host.data_dir).expect("catalog")),
            chat: Arc::clone(&host.chat),
            seat: Arc::clone(&host.seat),
            hook_token: Arc::new(host.hook_token.clone()),
            mail: Arc::new(crate::harness::test_mail(
                &host.data_dir,
                Arc::clone(&host.chat),
                crate::harness::mail_transport(None),
            )),
            voice: Arc::new(hn_core_api::voice::VoiceService::new(None)),
            alfred: Arc::clone(&host.alfred) as Arc<dyn AlfredBackend>,
            pairing_audit: Arc::new(PairingNotice::new(&host.data_dir, Arc::clone(&host.chat))),
            audit: Arc::new(hn_core_host::audit::AuditLog::new(&host.data_dir)),
            docs: Arc::new(hn_core_host::craft::Craft::without_tools(&host.data_dir)),
        })
        .with_host_pin(Some(pin));
        let base = Instant::now();
        let clock = Arc::new(Mutex::new(Duration::ZERO));
        let seen = Arc::clone(&clock);
        ctx.pake_slot = Arc::new(PakeSlot::with_clock(Arc::new(move || {
            base + *seen.lock().expect("clock")
        })));
        tokio::spawn(serve_client(listener, ctx.clone()));
        let plain_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind plain");
        let plain = plain_listener.local_addr().expect("plain address");
        tokio::spawn(serve(plain_listener, ctx.clone()));
        Rig {
            lan,
            plain,
            pins: slots.pins().expect("pins"),
            ctx,
            clock,
            host_id: hex::encode(pin.as_bytes()),
            host,
        }
    }

    pub async fn connect(&self) -> Conn {
        connect(self.lan, self.pins).await
    }

    pub fn advance(&self, by: Duration) {
        *self.clock.lock().expect("clock") += by;
    }

    /// The operator's step: `POST /v1/admin/pake-code` on loopback with the host token.
    pub async fn arm(&self) -> Code {
        let (status, body) = plain_post(self.plain, ADMIN, Some(&self.host.hook_token)).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expires_in"], 180);
        Code::parse(body["code"].as_str().expect("code")).expect("a well-formed code")
    }

    pub fn audit(&self) -> Vec<Value> {
        audit_lines(&self.host.data_dir)
    }

    pub fn events(&self) -> Vec<(String, String)> {
        self.audit()
            .iter()
            .map(|line| {
                (
                    line["outcome"].as_str().expect("outcome").to_owned(),
                    line["via"].as_str().expect("via").to_owned(),
                )
            })
            .collect()
    }
}

pub fn audit_lines(data_dir: &Path) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(data_dir.join(AUDIT_FILE)) else {
        return Vec::new();
    };
    text.lines()
        .map(|line| serde_json::from_str(line).expect("one JSON line"))
        .collect()
}

/// One plaintext request on the loopback router; `token` goes in `X-HN-Hook-Token`.
pub async fn plain_post(addr: SocketAddr, path: &str, token: Option<&str>) -> (u16, Value) {
    plain_post_body(addr, path, token, "").await
}

pub async fn plain_post_body(
    addr: SocketAddr,
    path: &str,
    token: Option<&str>,
    body: &str,
) -> (u16, Value) {
    let header = token
        .map(|token| format!("X-HN-Hook-Token: {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{header}\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let (status, text) = raw_exchange(addr, request.as_bytes()).await;
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// A TLS connection of the test device, with what it can read from it: the RFC 9266 exporter and the SPKI hash of
/// the certificate the host presented.
pub struct Conn {
    pub tls: TlsStream<TcpStream>,
    pub exporter: [u8; 32],
    pub spki: [u8; 32],
}

pub async fn connect(addr: SocketAddr, pins: PinSet) -> Conn {
    let config = client_config(pins).expect("client config");
    let tcp = TcpStream::connect(addr).await.expect("connect");
    let name = ServerName::try_from("herdr-notify.lan")
        .expect("name")
        .to_owned();
    let tls = TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
        .expect("handshake");
    let connection = tls.get_ref().1;
    let mut exporter = [0u8; 32];
    connection
        .export_keying_material(&mut exporter, b"EXPORTER-Channel-Binding", None)
        .expect("client exporter");
    let leaf = connection
        .peer_certificates()
        .and_then(|chain| chain.first())
        .expect("the host certificate");
    let spki = *CertificateInfo::read(leaf.as_ref())
        .expect("certificate")
        .fingerprint()
        .as_bytes();
    Conn {
        tls,
        exporter,
        spki,
    }
}

impl Conn {
    /// A JSON POST on this connection, which stays open for the next request.
    pub async fn post(&mut self, path: &str, body: &Value) -> (u16, Value) {
        let payload = body.to_string();
        self.raw(&format!(
            "POST {path} HTTP/1.1\r\nHost: lan\r\nContent-Type: application/json\r\n\
             Content-Length: {}\r\n\r\n{payload}",
            payload.len()
        ))
        .await
    }

    pub async fn raw(&mut self, request: &str) -> (u16, Value) {
        self.tls.write_all(request.as_bytes()).await.expect("write");
        let mut buffer = Vec::new();
        loop {
            if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&buffer[..end]).to_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .map(|value| value.trim().parse::<usize>().expect("content length"))
                    .unwrap_or(0);
                if buffer.len() >= end + 4 + length {
                    let status = head
                        .split_whitespace()
                        .nth(1)
                        .and_then(|code| code.parse().ok())
                        .expect("status");
                    let body = &buffer[end + 4..end + 4 + length];
                    return (status, serde_json::from_slice(body).unwrap_or(Value::Null));
                }
            }
            let mut chunk = [0u8; 4096];
            let read = self.tls.read(&mut chunk).await.expect("read");
            assert!(read > 0, "the host closed the connection early");
            buffer.extend_from_slice(&chunk[..read]);
        }
    }
}

/// The test device: a P-256 public key, raw and as `/v1/pair` writes it.
pub struct Device {
    pub b64: String,
    pub raw: Vec<u8>,
}

pub fn device() -> Device {
    let b64 = DemoIdentity::generate("", "pake-phone").public_key_b64();
    let raw = STANDARD.decode(&b64).expect("base64 key");
    Device { b64, raw }
}

/// What the device learns from `start`.
pub struct Started {
    pub status: u16,
    pub answer: Value,
    pub key: Option<SessionKey>,
    pub host_tag: Vec<u8>,
}

impl Started {
    /// Would the device accept the host's tag if the connection it was sent on had this exporter and this SPKI?
    pub fn host_tag_ok(&self, exporter: &[u8; 32], spki: &[u8; 32]) -> bool {
        self.key.as_ref().is_some_and(|key| {
            key.verify_confirm(Role::Host, exporter, spki, &self.host_tag)
                .is_ok()
        })
    }
}

pub fn device_message(rig_host_id: &str, device: &Device, code: &Code) -> (Session, Vec<u8>) {
    Session::start(Role::Device, code, rig_host_id, &device.raw).expect("device start")
}

pub fn start_body(device: &Device, message: &[u8]) -> Value {
    json!({ "device_pubkey": device.b64, "message": STANDARD.encode(message) })
}

/// The device side of `start`, over `conn`, with `host_id` taken from the certificate `conn` saw.
pub async fn start(conn: &mut Conn, device: &Device, code: &Code) -> Started {
    let host_id = hex::encode(conn.spki);
    let (session, message) = device_message(&host_id, device, code);
    let (status, answer) = conn.post(START, &start_body(device, &message)).await;
    let mut key = None;
    let mut host_tag = Vec::new();
    if status == 200 {
        let host_message = STANDARD
            .decode(answer["message"].as_str().expect("message"))
            .expect("base64 message");
        key = Some(session.finish(&host_message).expect("host message"));
        host_tag = STANDARD
            .decode(answer["tag"].as_str().expect("tag"))
            .expect("base64 tag");
    }
    Started {
        status,
        answer,
        key,
        host_tag,
    }
}

/// The device's confirmation, bound to `exporter` and sent over `conn`.
pub async fn finish_for(
    conn: &mut Conn,
    key: &SessionKey,
    exporter: &[u8; 32],
    spki: &[u8; 32],
) -> (u16, Value) {
    let tag = key
        .confirm_tag(Role::Device, exporter, spki)
        .expect("device tag");
    conn.post(FINISH, &json!({ "tag": STANDARD.encode(tag) }))
        .await
}

pub async fn finish(conn: &mut Conn, key: &SessionKey) -> (u16, Value) {
    let (exporter, spki) = (conn.exporter, conn.spki);
    finish_for(conn, key, &exporter, &spki).await
}

pub fn detail(answer: &Value) -> &str {
    answer["detail"].as_str().unwrap_or_default()
}
