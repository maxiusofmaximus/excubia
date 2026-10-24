use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use herdr_desktop_core::identity::DeviceIdentity;
use herdr_desktop_core::state::{StateSnapshot, StateStore};
use herdr_desktop_core::stream::{run_event_loop, ConnectionStatus, EventSink, StreamOptions};
use herdr_desktop_core::GatewayClient;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{timeout, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct RecordingSink {
    connections: Mutex<Vec<&'static str>>,
    states: Mutex<Vec<u64>>,
    chats: Mutex<Vec<String>>,
}

#[async_trait]
impl EventSink for RecordingSink {
    async fn connection(&self, status: ConnectionStatus) {
        let kind = match status {
            ConnectionStatus::Idle => "idle",
            ConnectionStatus::Connecting => "connecting",
            ConnectionStatus::Connected { .. } => "connected",
            ConnectionStatus::Offline { .. } => "offline",
            ConnectionStatus::Fatal { .. } => "fatal",
        };
        self.connections.lock().unwrap().push(kind);
    }

    async fn state(&self, state: StateSnapshot) {
        self.states.lock().unwrap().push(state.revision);
    }

    async fn chat(&self, event: herdr_desktop_core::ChatEvent) {
        self.chats.lock().unwrap().push(format!(
            "{}:{}",
            event.event,
            event.conversation_id.unwrap_or_default()
        ));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnects_after_a_closed_stream_and_last_revision_wins() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));

    let server_attempts = attempts.clone();
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let attempt = server_attempts.fetch_add(1, Ordering::SeqCst);
            read_headers(&mut socket).await;
            let frame = if attempt == 0 {
                state_frame(1, "w1:p1")
            } else {
                state_frame(2, "w2:p1")
            };
            write_sse(&mut socket, &frame).await;
            if attempt == 0 {
                let _ = socket.shutdown().await;
            } else {
                // Keep the second stream open so the test observes convergence
                // without racing a third reconnect.
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        }
    });

    let identity = DeviceIdentity::generate("dev-test", "test");
    let client = Arc::new(GatewayClient::new(&format!("http://{address}"), identity).unwrap());
    let store = Arc::new(Mutex::new(StateStore::default()));
    let sink = Arc::new(RecordingSink::default());
    let cancel = CancellationToken::new();

    let task = tokio::spawn(run_event_loop(
        client,
        store.clone(),
        sink.clone(),
        cancel.clone(),
        StreamOptions {
            retry_base: Duration::from_millis(20),
            retry_max: Duration::from_millis(60),
            idle_timeout: Duration::from_secs(3),
        },
    ));

    wait_until("two states", || sink.states.lock().unwrap().len() >= 2).await;
    assert_eq!(store.lock().unwrap().revision(), Some(2));
    assert_eq!(
        store.lock().unwrap().snapshot().unwrap().agents[0].pane_id,
        "w2:p1"
    );
    assert!(
        sink.connections
            .lock()
            .unwrap()
            .iter()
            .filter(|kind| **kind == "connected")
            .count()
            >= 2,
        "the supervisor must have reconnected"
    );

    cancel.cancel();
    timeout(Duration::from_secs(2), task)
        .await
        .expect("the loop must stop on cancellation")
        .unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_chat_frame_reaches_the_sink_and_is_not_a_state_refresh() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_headers(&mut socket).await;
        let frame = "id: 58\nevent: chat.message\ndata: {\"event\":\"chat.message\",\"revision\":58,\"conversation_id\":\"cnv_1\",\"message_id\":\"msg_1\",\"seq\":2,\"change\":\"created\"}\n\n";
        write_sse(&mut socket, frame).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let identity = DeviceIdentity::generate("dev-test", "test");
    let client = Arc::new(GatewayClient::new(&format!("http://{address}"), identity).unwrap());
    let store = Arc::new(Mutex::new(StateStore::default()));
    let sink = Arc::new(RecordingSink::default());
    let cancel = CancellationToken::new();
    let task = tokio::spawn(run_event_loop(
        client,
        store,
        sink.clone(),
        cancel.clone(),
        StreamOptions {
            retry_base: Duration::from_millis(20),
            retry_max: Duration::from_millis(60),
            idle_timeout: Duration::from_secs(3),
        },
    ));

    wait_until("chat frame", || !sink.chats.lock().unwrap().is_empty()).await;
    assert_eq!(sink.chats.lock().unwrap()[0], "chat.message:cnv_1");
    assert!(
        sink.states.lock().unwrap().is_empty(),
        "un chat.* no vuelve a pedir el estado de agentes"
    );

    cancel.cancel();
    let _ = timeout(Duration::from_secs(2), task).await;
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_401_is_terminal_and_is_not_retried() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));

    let server_attempts = attempts.clone();
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            server_attempts.fetch_add(1, Ordering::SeqCst);
            read_headers(&mut socket).await;
            let body = r#"{"detail":"unauthorized"}"#;
            let response = format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }
    });

    let identity = DeviceIdentity::generate("dev-test", "test");
    let client = Arc::new(GatewayClient::new(&format!("http://{address}"), identity).unwrap());
    let store = Arc::new(Mutex::new(StateStore::default()));
    let sink = Arc::new(RecordingSink::default());
    let cancel = CancellationToken::new();

    let task = tokio::spawn(run_event_loop(
        client,
        store,
        sink.clone(),
        cancel,
        StreamOptions {
            retry_base: Duration::from_millis(20),
            retry_max: Duration::from_millis(60),
            idle_timeout: Duration::from_secs(3),
        },
    ));

    wait_until("fatal status", || {
        sink.connections.lock().unwrap().contains(&"fatal")
    })
    .await;
    timeout(Duration::from_secs(2), task)
        .await
        .expect("a 401 must stop the loop instead of retrying")
        .unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    server.abort();
}

async fn wait_until(label: &str, predicate: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timeout esperando {label}");
}

async fn read_headers(socket: &mut TcpStream) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        let read = socket.read(&mut chunk).await.unwrap_or(0);
        if read == 0 {
            return;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            return;
        }
    }
}

async fn write_sse(socket: &mut TcpStream, frame: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\r\n{frame}"
    );
    socket.write_all(response.as_bytes()).await.unwrap();
    socket.flush().await.unwrap();
}

fn state_frame(revision: u64, pane_id: &str) -> String {
    let payload = json!({
        "event": "state",
        "revision": revision,
        "state": {
            "revision": revision,
            "herdr_version": "0.9.1",
            "counts": {"idle": 1},
            "agents": [{
                "pane_id": pane_id,
                "project": "test",
                "agent": "opencode",
                "status": "idle",
                "attention": 0,
                "title": null
            }]
        }
    });
    format!("event: state\ndata: {payload}\n\n")
}
