//! The real runner: `std::process::Command` with no shell, a clean environment,
//! capped output and a time limit that kills the process. On Windows the CLI also
//! runs in a Job Object (see `job.rs`): it cannot start children, cannot use more
//! than a set amount of memory and dies with the host.

use crate::runner::{Captured, Invocation, Runner, MAX_OUTPUT_BYTES};
use crate::{App, CraftError};
#[cfg(windows)]
use crate::job::{isolate, Job};
use std::env;
use std::io::{self, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

/// Start the CLI without a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Start the CLI paused, so it cannot run before it is inside its job.
#[cfg(windows)]
const CREATE_SUSPENDED: u32 = 0x0000_0004;

/// Off Windows there is no job to put the CLI in.
#[cfg(not(windows))]
struct Job;

#[cfg(not(windows))]
fn isolate(_child: &Child) -> io::Result<Job> {
    Ok(Job)
}

pub struct ProcessRunner;

impl Runner for ProcessRunner {
    fn run(&self, inv: &Invocation) -> Result<Captured, CraftError> {
        execute(inv, &mut |_| {})
    }
}

/// `on_start` gets the CLI's pid once it is inside its job; the tests use it.
fn execute(inv: &Invocation, on_start: &mut dyn FnMut(u32)) -> Result<Captured, CraftError> {
    let (mut child, _job) = start(inv)?;
    on_start(child.id());
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

/// Starts the CLI already inside its job. If the job cannot be set up, the CLI is
/// killed: it never runs outside the sandbox.
fn start(inv: &Invocation) -> Result<(Child, Job), CraftError> {
    let mut child = spawn(inv)?;
    match isolate(&child) {
        Ok(job) => Ok((child, job)),
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(CraftError::Isolation(inv.app))
        }
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
    start_held(&mut command);
    command
        .spawn()
        .map_err(|err| CraftError::Spawn(inv.app, err))
}

#[cfg(windows)]
fn start_held(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
}

#[cfg(not(windows))]
fn start_held(_command: &mut Command) {}

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

/// Windows: the reader really is held by its job. The test CLIs are system
/// programs that need no document, so these run on any Windows PC.
#[cfg(all(test, windows))]
mod isolation {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    /// Second process started by `cmd`; prints SPAWNED only if it ran.
    const SECOND_PROCESS: &[&str] = &["/D", "/C", r"%SystemRoot%\System32\whoami.exe&&echo SPAWNED"];

    fn system_exe(name: &str) -> PathBuf {
        let root = env::var_os("SystemRoot").expect("SystemRoot is set on Windows");
        PathBuf::from(root).join("System32").join(name)
    }

    fn invocation(program: PathBuf, args: &[&str], timeout: Duration) -> Invocation {
        Invocation {
            app: App::WordCraft,
            program,
            args: args.iter().map(OsString::from).collect(),
            cwd: env::temp_dir(),
            timeout,
        }
    }

    /// True while process `pid` has not exited.
    fn alive(pid: u32) -> bool {
        // SAFETY: a query on `pid`; a null handle means the process no longer exists.
        let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if raw.is_null() {
            return false;
        }
        let running = unsafe { WaitForSingleObject(raw, 0) } == WAIT_TIMEOUT;
        unsafe { CloseHandle(raw) };
        running
    }

    fn said_spawned(out: &[u8]) -> bool {
        String::from_utf8_lossy(out).contains("SPAWNED")
    }

    #[test]
    fn the_same_second_process_runs_outside_a_job() {
        let out = Command::new(system_exe("cmd.exe"))
            .args(SECOND_PROCESS)
            .output()
            .unwrap();
        assert!(said_spawned(&out.stdout), "the control command must work");
    }

    #[test]
    fn a_reader_cannot_start_a_second_process() {
        let inv = invocation(system_exe("cmd.exe"), SECOND_PROCESS, Duration::from_secs(30));
        let got = ProcessRunner.run(&inv).unwrap();
        assert!(!said_spawned(&got.stdout));
        assert!(!got.success);
    }

    #[test]
    fn the_reader_is_gone_after_a_normal_run() {
        let inv = invocation(system_exe("whoami.exe"), &[], Duration::from_secs(30));
        let mut pid = 0;
        let got = execute(&inv, &mut |p| pid = p).unwrap();
        assert!(got.success);
        assert!(pid != 0 && !alive(pid));
    }

    #[test]
    fn a_reader_that_runs_out_of_time_is_killed() {
        let inv = invocation(
            system_exe("PING.EXE"),
            &["-n", "60", "127.0.0.1"],
            Duration::from_millis(500),
        );
        let mut pid = 0;
        let err = execute(&inv, &mut |p| pid = p).unwrap_err();
        assert!(matches!(err, CraftError::Timeout(App::WordCraft)));
        assert!(pid != 0 && !alive(pid));
    }

    #[test]
    fn closing_the_job_kills_what_is_still_inside() {
        let inv = invocation(
            system_exe("PING.EXE"),
            &["-n", "60", "127.0.0.1"],
            Duration::from_secs(60),
        );
        let (mut child, job) = start(&inv).unwrap();
        drop(job);
        let deadline = Instant::now() + Duration::from_secs(10);
        let exited = loop {
            if child.try_wait().unwrap().is_some() {
                break true;
            }
            if Instant::now() > deadline {
                break false;
            }
            thread::sleep(Duration::from_millis(50));
        };
        let _ = child.kill();
        assert!(exited, "the reader outlived its job");
    }
}
