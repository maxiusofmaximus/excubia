//! The real runner: `std::process::Command` with no shell, a clean environment,
//! capped output and a time limit that kills the process.

use crate::runner::{Captured, Invocation, Runner, MAX_OUTPUT_BYTES};
use crate::{App, CraftError};
use std::env;
use std::io::{self, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

/// Start the CLI without a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct ProcessRunner;

impl Runner for ProcessRunner {
    fn run(&self, inv: &Invocation) -> Result<Captured, CraftError> {
        let mut child = spawn(inv)?;
        let (tx, rx) = mpsc::channel();
        collect(child.stdout.take(), Stream::Out, tx.clone());
        collect(child.stderr.take(), Stream::Err, tx);
        let gathered = gather(&rx, Instant::now() + inv.timeout, inv.app);
        let (out, err) = match gathered {
            Ok(pair) => pair,
            Err(failure) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(failure);
            }
        };
        let status = child.wait()?;
        Ok(Captured {
            success: status.success(),
            stdout: out.bytes,
            stderr: err.bytes,
            truncated: out.truncated || err.truncated,
        })
    }
}

fn spawn(inv: &Invocation) -> Result<Child, CraftError> {
    let mut command = Command::new(&inv.program);
    command
        .args(&inv.args)
        .current_dir(&inv.cwd)
        .env_clear()
        .env("TEMP", &inv.cwd)
        .env("TMP", &inv.cwd)
        .env("PATH", "")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(root) = env::var_os("SystemRoot") {
        command.env("SystemRoot", root);
    }
    hide_window(&mut command);
    command
        .spawn()
        .map_err(|err| CraftError::Spawn(inv.app, err))
}

#[cfg(windows)]
fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut Command) {}

#[derive(Debug, Default)]
struct Capture {
    bytes: Vec<u8>,
    truncated: bool,
}

#[derive(Clone, Copy)]
enum Stream {
    Out,
    Err,
}

type Piped = (Stream, io::Result<Capture>);

/// Reads one pipe on its own thread; the reader ends when the pipe closes.
fn collect<R: Read + Send + 'static>(pipe: Option<R>, which: Stream, tx: Sender<Piped>) {
    thread::spawn(move || {
        let result = match pipe {
            Some(pipe) => read_capped(pipe),
            None => Ok(Capture::default()),
        };
        let _ = tx.send((which, result));
    });
}

/// Keeps the first `MAX_OUTPUT_BYTES` and keeps reading the rest, so the child
/// never blocks on a full pipe.
fn read_capped(mut pipe: impl Read) -> io::Result<Capture> {
    let mut bytes = Vec::new();
    let mut truncated = false;
    let mut chunk = [0u8; 8192];
    loop {
        let read = pipe.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let room = MAX_OUTPUT_BYTES - bytes.len();
        bytes.extend_from_slice(&chunk[..read.min(room)]);
        truncated |= read > room;
    }
    Ok(Capture { bytes, truncated })
}

/// Waits for both pipes to finish, or for the deadline.
fn gather(
    rx: &Receiver<Piped>,
    deadline: Instant,
    app: App,
) -> Result<(Capture, Capture), CraftError> {
    let mut out = Capture::default();
    let mut err = Capture::default();
    for _ in 0..2 {
        let (which, result) = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| CraftError::Timeout(app))?;
        let capture = result?;
        match which {
            Stream::Out => out = capture,
            Stream::Err => err = capture,
        }
    }
    Ok((out, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_kept_up_to_the_cap_and_marked_truncated() {
        let big = vec![7u8; MAX_OUTPUT_BYTES + 10];
        let got = read_capped(big.as_slice()).unwrap();
        assert_eq!(got.bytes.len(), MAX_OUTPUT_BYTES);
        assert!(got.truncated);
    }

    #[test]
    fn output_of_exactly_the_cap_is_not_truncated() {
        let exact = vec![1u8; MAX_OUTPUT_BYTES];
        let got = read_capped(exact.as_slice()).unwrap();
        assert_eq!(got.bytes.len(), MAX_OUTPUT_BYTES);
        assert!(!got.truncated);
    }

    #[test]
    fn short_output_is_kept_whole() {
        let got = read_capped(&b"hola"[..]).unwrap();
        assert_eq!(got.bytes, b"hola");
        assert!(!got.truncated);
    }
}
