//! The local model provider of the UAC canary.
//!
//! The preflight must never spend quota, so the OpenCode canary talks to this
//! in-process server instead of a real model: it speaks the same
//! OpenAI-compatible protocol as `tooling/opencode-uac-plugin/e2e/mock-provider.mjs`
//! and always asks for one tool call, which is what the plugin has to block.
//! It is deliberately tiny: no dependency, one route, one answer.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use serde_json::json;

/// The largest request the provider reads before answering.
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

/// The provider: its base URL and the task that serves it. Dropping the value
/// stops the server.
pub struct CanaryProvider {
    base_url: String,
    task: JoinHandle<()>,
}

impl CanaryProvider {
    /// Start the provider on a free loopback port. `command` is what the
    /// simulated model asks to run.
    pub async fn start(command: &str) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        let turns = Arc::new(AtomicUsize::new(0));
        let task = tokio::spawn(serve(listener, command.to_string(), turns));
        Ok(Self {
            base_url: format!("http://127.0.0.1:{port}/v1"),
            task,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl Drop for CanaryProvider {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(listener: TcpListener, command: String, turns: Arc<AtomicUsize>) {
    while let Ok((stream, _)) = listener.accept().await {
        let command = command.clone();
        let turns = Arc::clone(&turns);
        tokio::spawn(async move {
            let _ = answer(stream, &command, &turns).await;
        });
    }
}

async fn answer(mut stream: TcpStream, command: &str, turns: &AtomicUsize) -> std::io::Result<()> {
    let request = read_request(&mut stream).await?;
    let body: serde_json::Value =
        serde_json::from_slice(&request.body).unwrap_or(serde_json::Value::Null);
    // The auxiliary calls of the CLI (a session title) carry no tools: only the
    // agent's own turns consume the one step of the script.
    let with_tools = body
        .get("tools")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|tools| !tools.is_empty());
    let streaming = body
        .get("stream")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let step = if with_tools && turns.fetch_add(1, Ordering::SeqCst) == 0 {
        Step::Command(command.to_string())
    } else {
        Step::Text
    };
    let payload = completion(&body, &step);
    if streaming {
        write_stream(&mut stream, &payload).await
    } else {
        write_json(&mut stream, &payload).await
    }
}

/// One read of the HTTP request: the head and, when the client sent a length,
/// the whole body.
struct Request {
    body: Vec<u8>,
}

async fn read_request(stream: &mut TcpStream) -> std::io::Result<Request> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(head_end) = find(&buffer, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buffer[..head_end]).to_lowercase();
            let length = content_length(&head).unwrap_or(0);
            if buffer.len() >= head_end + 4 + length {
                let body = buffer[head_end + 4..head_end + 4 + length].to_vec();
                return Ok(Request { body });
            }
        }
        if buffer.len() > MAX_REQUEST_BYTES {
            break;
        }
    }
    Ok(Request { body: Vec::new() })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn content_length(head: &str) -> Option<usize> {
    head.lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
}

/// What the simulated model does in one turn.
enum Step {
    /// Ask for the canary command, which the plugin must block.
    Command(String),
    /// End the turn: the CLI has nothing else to do.
    Text,
}

fn completion(body: &serde_json::Value, step: &Step) -> serde_json::Value {
    let model = body
        .get("model")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("mock");
    let message = match step {
        Step::Command(command) => json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "call_uac_1",
                "type": "function",
                "function": {
                    "name": "bash",
                    "arguments": json!({ "command": command }).to_string(),
                },
            }],
        }),
        Step::Text => json!({ "role": "assistant", "content": "fin" }),
    };
    json!({
        "id": "chatcmpl_canary",
        "object": "chat.completion",
        "created": 0,
        "model": model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": match step { Step::Command(_) => "tool_calls", Step::Text => "stop" },
        }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 },
    })
}

async fn write_json(stream: &mut TcpStream, payload: &serde_json::Value) -> std::io::Result<()> {
    let body = payload.to_string();
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.shutdown().await
}

/// The streamed shape the SDKs expect: the delta, then the finish with usage,
/// then `[DONE]`.
async fn write_stream(stream: &mut TcpStream, payload: &serde_json::Value) -> std::io::Result<()> {
    let id = payload["id"].clone();
    let model = payload["model"].clone();
    let choice = &payload["choices"][0];
    let base = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": 0,
        "model": model,
    });
    let mut first = base.clone();
    first["choices"] = json!([{
        "index": 0,
        "delta": choice["message"].clone(),
        "finish_reason": null,
    }]);
    let mut last = base;
    last["choices"] = json!([{
        "index": 0,
        "delta": {},
        "finish_reason": choice["finish_reason"].clone(),
    }]);
    last["usage"] = payload["usage"].clone();
    let body = format!("data: {first}\n\ndata: {last}\n\ndata: [DONE]\n\n");
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.shutdown().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_length_is_read_from_the_head() {
        let head = "post /v1/chat/completions http/1.1\r\ncontent-length: 42\r\n";
        assert_eq!(content_length(head), Some(42));
        assert_eq!(content_length("post / HTTP/1.1\r\n"), None);
    }

    #[test]
    fn the_completion_asks_for_the_command() {
        let step = Step::Command("echo hi".to_string());
        let payload = completion(&json!({ "model": "mock/mock" }), &step);
        assert_eq!(
            payload["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
            "bash"
        );
        let arguments = payload["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .expect("arguments");
        let arguments: serde_json::Value = serde_json::from_str(arguments).expect("json");
        assert_eq!(arguments["command"], "echo hi");
        assert_eq!(payload["choices"][0]["finish_reason"], "tool_calls");
    }

    #[test]
    fn the_text_step_closes_the_turn() {
        let payload = completion(&json!({}), &Step::Text);
        assert_eq!(payload["choices"][0]["message"]["content"], "fin");
        assert_eq!(payload["choices"][0]["finish_reason"], "stop");
    }
}
