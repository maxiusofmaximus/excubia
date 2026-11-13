//! Runs the decode in `excubia-media-worker`, so a crash, hang or memory blow-up stays in the child.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use crate::error::MediaError;
use crate::limits::Limits;
use crate::process::ProcessedImage;
use crate::wire::parse_reply;
use crate::worker::limits_args;

/// A valid reply is at most a 2048-pixel-side WebP plus its header; this caps a runaway worker.
const MAX_REPLY_BYTES: u64 = 32 * 1024 * 1024;

pub fn process_image_isolated(
    input: &[u8],
    limits: Limits,
    timeout: Duration,
) -> Result<ProcessedImage, MediaError> {
    if input.len() > limits.max_input_bytes {
        return Err(MediaError::InputTooLarge);
    }
    let frame = run_worker(input, limits, timeout)?;
    parse_reply(&frame)
}

fn run_worker(input: &[u8], limits: Limits, timeout: Duration) -> Result<Vec<u8>, MediaError> {
    let mut child = spawn_worker(limits)?;
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        stop(&mut child);
        return Err(MediaError::WorkerFailed);
    };
    let (sender, receiver) = mpsc::channel();
    thread::scope(|scope| {
        // A worker that exits early closes its stdin; the failed write is reported by its exit status.
        scope.spawn(move || {
            let _ = stdin.write_all(input);
        });
        scope.spawn(move || {
            let _ = sender.send(read_capped(stdout));
        });
        match receiver.recv_timeout(timeout) {
            Ok(Ok(frame)) => finish(&mut child, frame),
            Ok(Err(_)) => fail(&mut child, MediaError::WorkerFailed),
            Err(RecvTimeoutError::Timeout) => fail(&mut child, MediaError::Timeout),
            Err(RecvTimeoutError::Disconnected) => fail(&mut child, MediaError::WorkerFailed),
        }
    })
}

fn spawn_worker(limits: Limits) -> Result<Child, MediaError> {
    Command::new(worker_path().map_err(|_| MediaError::WorkerFailed)?)
        .args(limits_args(limits))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| MediaError::WorkerFailed)
}

/// The worker sits next to this executable. Test binaries run from `target/<profile>/deps`,
/// where cargo does not put the worker, so that one level is skipped.
fn worker_path() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let mut dir = exe
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("executable has no directory"))?;
    if dir.ends_with("deps") {
        dir.pop();
    }
    Ok(dir.join(format!(
        "excubia-media-worker{}",
        std::env::consts::EXE_SUFFIX
    )))
}

fn read_capped(stdout: ChildStdout) -> io::Result<Vec<u8>> {
    let mut frame = Vec::new();
    stdout.take(MAX_REPLY_BYTES + 1).read_to_end(&mut frame)?;
    if frame.len() as u64 > MAX_REPLY_BYTES {
        return Err(io::Error::other("worker reply exceeds the limit"));
    }
    Ok(frame)
}

/// The reply arrived, so the worker has finished reading and writing; a non-zero exit is a fault.
fn finish(child: &mut Child, frame: Vec<u8>) -> Result<Vec<u8>, MediaError> {
    match child.wait() {
        Ok(status) if status.success() => Ok(frame),
        _ => Err(MediaError::WorkerFailed),
    }
}

fn fail<T>(child: &mut Child, error: MediaError) -> Result<T, MediaError> {
    stop(child);
    Err(error)
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
