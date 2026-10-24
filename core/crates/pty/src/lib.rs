//! PTY hosting for the herdr-notify core.
//!
//! One [`PtySession`] owns one pseudo-terminal with one CLI running inside it:
//! output streams out through a channel, input is written through the master
//! side, the size can be changed at any time and the child can be killed.
//! Windows uses ConPTY; Unix uses the native pty. Both come from
//! `portable-pty`, the same crate herdr pins (`=0.9.0`), minus herdr's
//! vendored patches (see NOTICE).

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Context, Result};
use portable_pty::{
    native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize as PortablePtySize,
};
use tokio::sync::mpsc;

/// Terminal geometry, in character cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PtySize {
    pub rows: u16,
    pub cols: u16,
}

impl Default for PtySize {
    fn default() -> Self {
        Self {
            rows: 30,
            cols: 100,
        }
    }
}

/// The command line to run inside the pty.
#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<std::path::PathBuf>,
    pub env: Vec<(String, String)>,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn cwd(mut self, cwd: impl Into<std::path::PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

/// How the child exited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitInfo {
    pub exit_code: u32,
    pub signal: Option<String>,
}

/// A shared handle to the pty's input side, for asynchronous writers (for
/// example the terminal answering a cursor-position query while the reader
/// thread is blocked).
#[derive(Clone)]
pub struct WriteHandle(Arc<Mutex<Box<dyn Write + Send>>>);

impl WriteHandle {
    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        let mut writer = self.0.lock().expect("pty writer poisoned");
        writer.write_all(bytes).context("pty write")?;
        writer.flush().context("pty flush")
    }
}

/// A live pty session.
pub struct PtySession {
    output: Option<mpsc::UnboundedReceiver<Vec<u8>>>,
    exit: Option<mpsc::UnboundedReceiver<ExitInfo>>,
    writer: WriteHandle,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    pid: Option<u32>,
}

impl PtySession {
    /// Spawn `cmd` in a fresh pty of `size`.
    pub fn spawn(cmd: CommandSpec, size: PtySize) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PortablePtySize {
                rows: size.rows,
                cols: size.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("openpty")?;

        let mut builder = CommandBuilder::new(&cmd.program);
        builder.args(&cmd.args);
        if let Some(cwd) = &cmd.cwd {
            builder.cwd(cwd);
        }
        for (key, value) in &cmd.env {
            builder.env(key, value);
        }

        let child = pair.slave.spawn_command(builder).context("spawn")?;
        drop(pair.slave);

        let killer = child.clone_killer();
        let pid = child.process_id();
        let reader = pair.master.try_clone_reader().context("clone reader")?;
        let writer = pair.master.take_writer().context("take writer")?;

        let (output_tx, output) = mpsc::unbounded_channel();
        let (exit_tx, exit) = mpsc::unbounded_channel();

        spawn_reader(reader, output_tx);
        spawn_waiter(child, exit_tx);

        Ok(Self {
            output: Some(output),
            exit: Some(exit),
            writer: WriteHandle(Arc::new(Mutex::new(writer))),
            master: Mutex::new(pair.master),
            killer: Mutex::new(killer),
            pid,
        })
    }

    /// Take the output stream. Available once; afterwards it returns `None`.
    pub fn take_output(&mut self) -> Option<mpsc::UnboundedReceiver<Vec<u8>>> {
        self.output.take()
    }

    /// Take the exit notification. Available once; afterwards it returns `None`.
    pub fn take_exit(&mut self) -> Option<mpsc::UnboundedReceiver<ExitInfo>> {
        self.exit.take()
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Write raw bytes to the terminal, as if typed.
    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        self.writer.write(bytes)
    }

    /// A cloneable handle to the same writer, usable from other tasks.
    pub fn write_handle(&self) -> WriteHandle {
        self.writer.clone()
    }

    /// Change the terminal size (the child gets SIGWINCH on Unix / a
    /// ConPTY resize on Windows).
    pub fn resize(&self, size: PtySize) -> Result<()> {
        let master = self.master.lock().expect("pty master poisoned");
        master
            .resize(PortablePtySize {
                rows: size.rows,
                cols: size.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("pty resize")
    }

    /// Terminate the child (and, on Windows, its console).
    pub fn kill(&self) -> Result<()> {
        let mut killer = self.killer.lock().expect("pty killer poisoned");
        killer.kill().context("pty kill")
    }

    /// Force the child to die when the orderly close does not take: `SIGKILL`
    /// on Unix; on Windows `kill` already terminates, so this repeats it.
    pub fn kill_force(&self) -> Result<()> {
        #[cfg(unix)]
        {
            if let Some(pid) = self.pid {
                // SAFETY: `pid` is the child this session started.
                let result = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
                if result != 0 {
                    return Err(std::io::Error::last_os_error()).context("pty kill -9");
                }
                return Ok(());
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            self.kill()
        }
    }
}

fn spawn_reader(mut reader: Box<dyn Read + Send>, output_tx: mpsc::UnboundedSender<Vec<u8>>) {
    let spawned = thread::Builder::new()
        .name("hn-pty-reader".to_string())
        .spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if output_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    if spawned.is_err() {
        // The session still works without a reader thread; output just never
        // arrives. Surface nothing here: spawn() already returned the handle.
    }
}

fn spawn_waiter(mut child: Box<dyn Child + Send + Sync>, exit_tx: mpsc::UnboundedSender<ExitInfo>) {
    let spawned = thread::Builder::new()
        .name("hn-pty-exit".to_string())
        .spawn(move || {
            let info = match child.wait() {
                Ok(status) => ExitInfo {
                    exit_code: status.exit_code(),
                    signal: status.signal().map(str::to_string),
                },
                Err(_) => ExitInfo {
                    exit_code: 1,
                    signal: None,
                },
            };
            let _ = exit_tx.send(info);
        });
    if spawned.is_err() {
        // Same as above: the exit notification is best effort.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn echo_spec() -> CommandSpec {
        if cfg!(windows) {
            let mut spec = CommandSpec::new("cmd.exe");
            spec.args.push("/d".into());
            spec.args.push("/c".into());
            spec.args.push("echo hola-pty".into());
            spec
        } else {
            let mut spec = CommandSpec::new("sh");
            spec.args.push("-c".into());
            spec.args.push("echo hola-pty".into());
            spec
        }
    }

    #[tokio::test]
    async fn spawn_reads_output_and_exit() {
        let mut session = PtySession::spawn(echo_spec(), PtySize::default()).expect("spawn");
        let mut output = session.take_output().expect("output");
        let mut exit_rx = session.take_exit().expect("exit");
        let mut seen = Vec::new();
        let mut exit = None;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            tokio::select! {
                chunk = output.recv() => match chunk {
                    Some(bytes) => {
                        // ConPTY (inherit-cursor) asks the terminal for the
                        // cursor position and waits for the reply before it
                        // streams the child's output.
                        if bytes.windows(4).any(|window| window == b"\x1b[6n") {
                            session.write(b"\x1b[1;1R").expect("cursor reply");
                        }
                        seen.extend_from_slice(&bytes);
                    }
                    None => break,
                },
                info = exit_rx.recv(), if exit.is_none() => {
                    exit = info;
                }
                _ = tokio::time::sleep_until(deadline) => panic!("pty test timed out"),
            }
            if exit.is_some() && seen.windows(8).any(|w| w == b"hola-pty") {
                break;
            }
        }

        let text = String::from_utf8_lossy(&seen);
        assert!(text.contains("hola-pty"), "output was: {text:?}");
        assert_eq!(exit.expect("exit event").exit_code, 0);
    }

    #[tokio::test]
    async fn resize_after_spawn_does_not_fail() {
        let session = PtySession::spawn(
            echo_spec(),
            PtySize {
                rows: 30,
                cols: 100,
            },
        )
        .expect("spawn");
        session
            .resize(PtySize {
                rows: 40,
                cols: 120,
            })
            .expect("resize");
        assert!(session.pid().is_some());
    }
}
