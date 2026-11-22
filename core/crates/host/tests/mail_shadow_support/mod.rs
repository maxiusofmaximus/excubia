//! A triage sidecar on loopback that answers what a test scripts, and keeps every
//! request body so a test can see the ficha that left the host.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone)]
pub enum Reply {
    /// 200 with this JSON.
    Json(Value),
    /// This status with this body, as it is.
    Raw(u16, &'static str),
    /// 200 with this JSON, after the delay.
    Slow(Duration, Value),
}

pub struct FakeSidecar {
    pub port: u16,
    pub bodies: Arc<Mutex<Vec<String>>>,
}

pub async fn serve(reply: Reply) -> FakeSidecar {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&bodies);
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(answer(stream, reply.clone(), Arc::clone(&seen)));
        }
    });
    FakeSidecar { port, bodies }
}

/// A loopback port that nothing listens on: the sidecar is down.
pub async fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    listener.local_addr().expect("address").port()
}

async fn answer(mut stream: TcpStream, reply: Reply, seen: Arc<Mutex<Vec<String>>>) {
    let body = read_body(&mut stream).await;
    seen.lock().expect("bodies").push(body);
    let (status, text, delay) = match reply {
        Reply::Json(json) => (200, json.to_string(), None),
        Reply::Raw(status, text) => (status, text.to_owned(), None),
        Reply::Slow(delay, json) => (200, json.to_string(), Some(delay)),
    };
    if let Some(delay) = delay {
        tokio::time::sleep(delay).await;
    }
    let head = format!(
        "HTTP/1.0 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        text.len()
    );
    // A client that gave up has closed the socket: its timeout is the outcome, not this write.
    let _ = stream.write_all(format!("{head}{text}").as_bytes()).await;
}

/// The body of one request: the head, then as many bytes as its Content-Length says.
async fn read_body(stream: &mut TcpStream) -> String {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(split) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if raw.len() >= split + 4 + length {
                return String::from_utf8_lossy(&raw[split + 4..split + 4 + length]).into_owned();
            }
        }
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return String::new(),
            Ok(read) => raw.extend_from_slice(&chunk[..read]),
        }
    }
}
