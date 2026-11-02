// Each test binary includes this module and uses only part of it.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

pub struct Captured {
    pub method: String,
    pub target: String,
    pub headers: String,
    pub body: Vec<u8>,
}

/// A fake host on loopback that answers `responses` in order and records
/// what it saw. Nothing leaves the machine.
pub async fn spawn_server(
    responses: Vec<(u16, &'static str)>,
) -> (String, Arc<Mutex<Vec<Captured>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let server_captured = captured.clone();

    tokio::spawn(async move {
        for (status, body) in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (headers, request_body) = read_request(&mut socket).await;
            let request_line = headers.lines().next().unwrap_or_default().to_string();
            let mut parts = request_line.split(' ');
            server_captured.lock().unwrap().push(Captured {
                method: parts.next().unwrap_or_default().to_string(),
                target: parts.next().unwrap_or_default().to_string(),
                headers: headers.clone(),
                body: request_body,
            });
            let response = format!(
                "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                reason(status),
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            let _ = socket.shutdown().await;
        }
    });

    (format!("http://{address}"), captured)
}

/// A fake host that answers one request with `status`, its own `headers` (each line
/// ending in CRLF) and a raw `body`, and records what it saw.
pub async fn spawn_reply(
    status: u16,
    headers: &'static str,
    body: &'static [u8],
) -> (String, Arc<Mutex<Option<Captured>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let captured = Arc::new(Mutex::new(None));
    let server_captured = captured.clone();

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let (head, request_body) = read_request(&mut socket).await;
        let mut parts = head.lines().next().unwrap_or_default().split(' ');
        let method = parts.next().unwrap_or_default().to_string();
        let target = parts.next().unwrap_or_default().to_string();
        *server_captured.lock().unwrap() = Some(Captured {
            method,
            target,
            headers: head,
            body: request_body,
        });
        let response = format!(
            "HTTP/1.1 {status} {}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
            reason(status),
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.write_all(body).await.unwrap();
        let _ = socket.shutdown().await;
    });

    (format!("http://{address}"), captured)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        _ => "Error",
    }
}

/// Reads one HTTP request: the head plus exactly `Content-Length` body bytes.
pub async fn read_request(socket: &mut TcpStream) -> (String, Vec<u8>) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let read = socket.read(&mut chunk).await.unwrap_or(0);
        assert!(read > 0, "client closed before sending headers");
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };

    let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let content_length: usize = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().ok())?
        })
        .unwrap_or(0);

    let mut body = buffer[header_end..].to_vec();
    while body.len() < content_length {
        let read = socket.read(&mut chunk).await.unwrap_or(0);
        assert!(read > 0, "client closed before sending the body");
        body.extend_from_slice(&chunk[..read]);
    }
    (headers, body)
}
