//! Pairing with the short code (F2d1, `core/docs/contracts/pairing.md` F2b): the
//! device side of `POST /v1/pair/pake/start` and `/finish`. The order is the
//! contract's: nothing of ours leaves before the host's tag verifies, and the
//! pin the device keeps is the SPKI that tag confirmed. The session and its key
//! never leave this function.

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use excubia_pake::{Code, Role, Session};
use excubia_pin::{client_config, Fingerprint, PinSet};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use super::capture::capture_config;
use super::transport::{
    open, parse_addresses, read_capped, send, Opened, Request, Tls, REQUEST_TIMEOUT,
};
use crate::api::pinned::{
    HeaderField, PakeFailure, PakeFailureKind as Kind, PinnedFailure,
    PinnedFailureKind as Transport,
};

const START: &str = "/v1/pair/pake/start";
const FINISH: &str = "/v1/pair/pake/finish";

pub(crate) struct Paired {
    pub(crate) device_id: String,
    pub(crate) recovery_code: String,
    pub(crate) pin: Fingerprint,
    pub(crate) address: SocketAddr,
}

#[derive(Deserialize)]
struct StartAnswer {
    message: String,
    tag: String,
}

#[derive(Deserialize)]
struct FinishAnswer {
    device_id: String,
    recovery_code: String,
}

#[derive(Deserialize)]
struct Refusal {
    detail: Option<Value>,
}

struct Reply {
    status: u16,
    retry_after: Option<u32>,
    body: Vec<u8>,
}

pub(crate) fn pair(
    addresses: &[String],
    code: &str,
    device_public_key_b64: &str,
    expected_host: Option<&str>,
) -> Result<Paired, PakeFailure> {
    let code = Code::parse(code).map_err(|_| {
        PakeFailure::of(
            Kind::InvalidCodeFormat,
            "El código tiene 8 caracteres: letras (sin I ni O) y cifras del 2 al 9.",
        )
    })?;
    let device_key = device_key(device_public_key_b64)?;
    let expected = expected_host.map(expected_pin).transpose()?;
    let addresses = parse_addresses(addresses).map_err(from_transport)?;

    // First connection: no pin exists yet; see `capture` for why that is safe.
    let capture = Arc::new(capture_config().map_err(from_transport)?);
    let first = open_expected(&addresses, &capture, expected)?;
    let (address, spki, first_exporter) = (first.address, first.spki, first.exporter);
    let host_id = hex::encode(spki.as_bytes());
    let started = Session::start(Role::Device, &code, &host_id, &device_key);
    drop(code);
    let (session, message) = started.map_err(|_| protocol("no se pudo iniciar el PAKE"))?;

    let body = json!({
        "device_pubkey": STANDARD.encode(&device_key),
        "message": STANDARD.encode(message),
    });
    let reply = post(first.tls, address, START, &body).map_err(from_transport)?;
    let started: StartAnswer = answer(reply)?;
    let key = session
        .finish(&decode(&started.message)?)
        .map_err(|_| protocol("mensaje PAKE del equipo inválido"))?;
    // Before anything of ours leaves: a wrong code or a relay stops here.
    key.verify_confirm(Role::Host, &first_exporter, spki.as_bytes(), &decode(&started.tag)?)
        .map_err(|_| {
            PakeFailure::of(
                Kind::HostProofInvalid,
                "El equipo no demostró conocer el código. Revísalo; si es correcto, alguien podría estar interceptando la conexión.",
            )
        })?;

    // Second connection: pinned to the SPKI just confirmed, same leaf or it fails.
    let pinned =
        client_config(PinSet::new(spki, None)).map_err(|error| protocol(&error.to_string()))?;
    let second = open(&[address], &Arc::new(pinned), REQUEST_TIMEOUT)
        .map_err(|failure| lost_midway(from_transport(failure)))?;
    let tag = key
        .confirm_tag(Role::Device, &second.exporter, spki.as_bytes())
        .map_err(|_| protocol("no se pudo confirmar el PAKE"))?;
    let reply = post(
        second.tls,
        address,
        FINISH,
        &json!({ "tag": STANDARD.encode(tag) }),
    )
    .map_err(|failure| lost_midway(from_transport(failure)))?;
    let done: FinishAnswer = answer(reply)?;
    Ok(Paired {
        device_id: done.device_id,
        recovery_code: done.recovery_code,
        pin: spki,
        address,
    })
}

/// The pin the QR carried (`h`), parsed before any network; the error never echoes it.
fn expected_pin(text: &str) -> Result<Fingerprint, PakeFailure> {
    Fingerprint::from_str(text)
        .map_err(|_| protocol("la huella del equipo esperada no tiene el formato debido"))
}

/// Each candidate is opened on its own. One that is not TLS, does not answer or (with
/// `expected`, the QR's pin) shows another key is skipped before a byte is written; the first
/// that passes is used. A mismatch outranks the rest (the QR may be another PC's).
fn open_expected(
    addresses: &[SocketAddr],
    config: &Arc<rustls::ClientConfig>,
    expected: Option<Fingerprint>,
) -> Result<Opened, PakeFailure> {
    let mut mismatch = false;
    let mut last = None;
    for address in addresses {
        match open(&[*address], config, REQUEST_TIMEOUT) {
            Ok(opened) if expected.is_none_or(|expected| opened.spki == expected) => {
                return Ok(opened)
            }
            Ok(_) => mismatch = true,
            Err(failure) => last = Some(failure),
        }
    }
    if mismatch {
        return Err(PakeFailure::of(
            Kind::HostMismatch,
            "Este PC no es el del QR. Vuelve a generar el QR en tu PC y escanéalo de nuevo.",
        ));
    }
    Err(from_transport(last.unwrap_or_else(|| {
        PinnedFailure::of(Transport::Unreachable, "no candidate answered")
    })))
}

/// The host validates the P-256 point before it spends the code; so do we.
fn device_key(device_public_key_b64: &str) -> Result<Vec<u8>, PakeFailure> {
    let bytes = STANDARD
        .decode(device_public_key_b64)
        .map_err(|_| protocol("la clave del dispositivo no es base64"))?;
    match p256::PublicKey::from_sec1_bytes(&bytes) {
        Ok(_) if bytes.len() == 65 => Ok(bytes),
        _ => Err(protocol(
            "la clave del dispositivo no es un punto P-256 sin comprimir",
        )),
    }
}

fn post(tls: Tls, address: SocketAddr, target: &str, body: &Value) -> Result<Reply, PinnedFailure> {
    let body = body.to_string().into_bytes();
    let headers = [
        HeaderField {
            name: "Content-Type".to_string(),
            value: "application/json".to_string(),
        },
        HeaderField {
            name: "Accept".to_string(),
            value: "application/json".to_string(),
        },
    ];
    let request = Request {
        method: "POST",
        target,
        headers: &headers,
        body: &body,
    };
    let mut exchange = send(tls, address, request)?;
    Ok(Reply {
        status: exchange.status,
        retry_after: exchange.retry_after,
        body: read_capped(&mut exchange.body)?,
    })
}

fn answer<T: DeserializeOwned>(reply: Reply) -> Result<T, PakeFailure> {
    if reply.status != 200 {
        return Err(refusal(&reply));
    }
    serde_json::from_slice(&reply.body).map_err(|_| protocol("respuesta del equipo ilegible"))
}

fn decode(text: &str) -> Result<Vec<u8>, PakeFailure> {
    STANDARD
        .decode(text)
        .map_err(|_| protocol("el equipo envió base64 inválido"))
}

/// The host's own error codes (`routes/pake.rs`) to the closed set the UI branches on.
fn refusal(reply: &Reply) -> PakeFailure {
    let code = serde_json::from_slice::<Refusal>(&reply.body)
        .ok()
        .and_then(|refusal| refusal.detail)
        .and_then(|detail| detail.as_str().map(str::to_owned))
        .unwrap_or_default();
    let (kind, message) = match (reply.status, code.as_str()) {
        (429, _) => (
            Kind::RateLimited,
            "Demasiados intentos seguidos. Espera un momento.".to_string(),
        ),
        (410, _) => (
            Kind::CodeGone,
            "El código caducó, ya se usó o fue anulado. Pide uno nuevo.".to_string(),
        ),
        (403, "confirmation_failed") => (
            Kind::HostProofInvalid,
            "El equipo rechazó la confirmación: código erróneo o conexión interceptada."
                .to_string(),
        ),
        (403, "pake_requires_tls") | (404, _) => (
            Kind::NoPake,
            "El equipo no admite emparejar con código por esta vía.".to_string(),
        ),
        (status, code) => {
            let code: String = code
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
                .take(40)
                .collect();
            (
                Kind::Protocol,
                format!("Respuesta inesperada del equipo ({status} {code})."),
            )
        }
    };
    PakeFailure {
        kind,
        message,
        retry_after: reply.retry_after,
    }
}

fn from_transport(failure: PinnedFailure) -> PakeFailure {
    let (kind, message) = match failure.kind {
        Transport::Unreachable | Transport::Refused | Transport::Timeout => (
            Kind::Unreachable,
            "No se pudo hablar con el equipo en la red.".to_string(),
        ),
        // Only the pinned second connection can fail this way: the leaf changed.
        Transport::PinMismatch => (
            Kind::ChannelChanged,
            "El equipo presentó otro certificado a mitad del emparejamiento; se canceló."
                .to_string(),
        ),
        _ => (
            Kind::Protocol,
            format!("Conexión no válida con el equipo ({}).", failure.detail),
        ),
    };
    PakeFailure {
        kind,
        message,
        retry_after: None,
    }
}

/// The code is spent once the host has answered `start`: a lost link now needs a new one.
fn lost_midway(failure: PakeFailure) -> PakeFailure {
    if failure.kind != Kind::Unreachable {
        return failure;
    }
    PakeFailure::of(
        Kind::Unreachable,
        "Se perdió la conexión a mitad del emparejamiento. El código ya no sirve: pide uno nuevo.",
    )
}

fn protocol(detail: &str) -> PakeFailure {
    PakeFailure::of(Kind::Protocol, format!("Error de protocolo: {detail}."))
}
