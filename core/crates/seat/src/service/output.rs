use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::task::JoinHandle;

use crate::output_tail::OutputTail;

use super::lock;

const READER_GRACE: Duration = Duration::from_millis(500);

/// Reads one CLI pipe into the turn's tail until it closes.
pub(super) fn capture<R>(mut stream: R, tail: Arc<Mutex<OutputTail>>) -> JoinHandle<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut chunk = [0_u8; 1024];
        loop {
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(read) => lock(&tail).push(&chunk[..read]),
            }
        }
    })
}

/// Gives the readers a short grace to reach end-of-pipe, so the tail holds the
/// last output. A grandchild may keep a pipe open; the grace bounds the wait.
pub(super) async fn settle(readers: [Option<JoinHandle<()>>; 2]) {
    for reader in readers.into_iter().flatten() {
        let _ = tokio::time::timeout(READER_GRACE, reader).await;
    }
}

/// The failure reason followed by the redacted output the CLI left behind.
pub(super) fn with_output(reason: String, excerpt: &str) -> String {
    if excerpt.is_empty() {
        return format!("{reason}; sin salida del CLI");
    }
    format!("{reason}; última salida: {excerpt}")
}
