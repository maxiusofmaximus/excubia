//! A simulated OpenAI-compatible scorer on loopback. It stands in for the
//! model: its answer is whatever behaviour the test picks, and every request is
//! counted, so a test can tell whether the scorer was asked at all. No model
//! quota is spent.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

/// What the simulated scorer answers.
#[derive(Clone, Copy)]
pub enum Behaviour {
    /// `{"risk_score": x}` for the command asked about.
    Scores(fn(&str) -> f64),
    /// The model's text, verbatim.
    Says(&'static str),
    /// An HTTP error status with no answer.
    Status(u16),
    /// A correct answer, after waiting this long.
    Sleeps(Duration),
}

pub struct Scorer {
    pub port: u16,
    pub hits: Arc<AtomicUsize>,
}

impl Scorer {
    pub fn start(behaviour: Behaviour) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind the simulated scorer");
        let port = listener.local_addr().expect("address").port();
        let hits = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&hits);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                counted.fetch_add(1, Ordering::SeqCst);
                serve(stream, behaviour);
            }
        });
        Self { port, hits }
    }

    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

/// A port with nothing listening: the connection is refused.
pub fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.local_addr().expect("address").port()
}

fn serve(stream: std::net::TcpStream, behaviour: Behaviour) {
    let mut reader = BufReader::new(stream);
    let mut length = 0usize;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);
    let command = command_of(&body);
    let (status, text) = match behaviour {
        Behaviour::Scores(score) => (200, json!({ "risk_score": score(&command) }).to_string()),
        Behaviour::Says(text) => (200, text.to_string()),
        Behaviour::Status(code) => (code, String::new()),
        Behaviour::Sleeps(wait) => {
            thread::sleep(wait);
            (200, json!({ "risk_score": 0.01 }).to_string())
        }
    };
    let answer =
        json!({ "choices": [{ "message": { "role": "assistant", "content": text } }] }).to_string();
    let body = if status == 200 { answer } else { String::new() };
    let mut stream = reader.into_inner();
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

/// The command inside the question the scorer was asked.
fn command_of(body: &[u8]) -> String {
    let request: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let question = request["messages"][1]["content"].as_str().unwrap_or("{}");
    let question: Value = serde_json::from_str(question).unwrap_or(Value::Null);
    question["command"].as_str().unwrap_or_default().to_string()
}
