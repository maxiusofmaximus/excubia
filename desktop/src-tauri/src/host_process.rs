//! The local Rust core (`hn-core-host`) as a managed child process.
//!
//! The daemon is spawned with an ephemeral port and reports the real one on
//! stdout, so nothing has to guess a free port. Liveness comes from the child's
//! exit event, not from polling: a dead host is restarted with backoff, and a
//! cancelled supervisor kills its child and returns.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

const RESTART_BASE: Duration = Duration::from_millis(500);
const RESTART_MAX: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    Started { pid: u32 },
    Ready { port: u16 },
    Exited { code: Option<i32> },
    Restarting { delay: Duration },
    Failed { error: String },
}

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub binary: PathBuf,
    /// Full daemon argument list, built by the caller so this module stays
    /// agnostic of the host CLI (and testable with any executable).
    pub args: Vec<String>,
    /// The data dir is only used by the CLI, never by the launcher.
    pub data_dir: PathBuf,
    pub log_path: PathBuf,
}

/// Runs the host until cancelled. The caller spawns it: this future must be
/// created inside a runtime, and the shell owns the task handle.
pub async fn supervise(
    config: HostConfig,
    events: UnboundedSender<HostEvent>,
    shutdown: CancellationToken,
) {
    run(config, events, shutdown).await
}

async fn run(config: HostConfig, events: UnboundedSender<HostEvent>, shutdown: CancellationToken) {
    let mut delay = RESTART_BASE;
    loop {
        if shutdown.is_cancelled() {
            return;
        }

        match launch(&config, &events).await {
            Ok(mut child) => {
                let _ = events.send(HostEvent::Started {
                    pid: child.id().unwrap_or_default(),
                });
                delay = RESTART_BASE;

                let status = tokio::select! {
                    status = child.wait() => Some(status),
                    _ = shutdown.cancelled() => {
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                        None
                    }
                };
                let Some(status) = status else { return };
                match status {
                    Ok(status) => {
                        let _ = events.send(HostEvent::Exited {
                            code: status.code(),
                        });
                    }
                    Err(error) => {
                        let _ = events.send(HostEvent::Failed {
                            error: error.to_string(),
                        });
                    }
                }
            }
            Err(error) => {
                let _ = events.send(HostEvent::Failed { error });
            }
        }

        if shutdown.is_cancelled() {
            return;
        }
        let _ = events.send(HostEvent::Restarting { delay });
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep(delay) => {}
        }
        delay = (delay * 2).min(RESTART_MAX);
    }
}

async fn launch(config: &HostConfig, events: &UnboundedSender<HostEvent>) -> Result<Child, String> {
    if let Some(parent) = config.log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut command = Command::new(&config.binary);
    command
        .args(&config.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|error| format!("no se pudo lanzar {}: {error}", config.binary.display()))?;
    if let Some(stdout) = child.stdout.take() {
        forward(stdout, config.log_path.clone(), Some(events.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        forward(stderr, config.log_path.clone(), None);
    }
    Ok(child)
}

/// Copies one child stream to the app log and reports the first "listening on"
/// line as ready. Lines are decoded lossily: a stray non-UTF-8 byte in a
/// child's banner must not stop the log or the readiness signal.
fn forward(
    stream: impl AsyncRead + Unpin + Send + 'static,
    log_path: PathBuf,
    events: Option<UnboundedSender<HostEvent>>,
) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(stream);
        let mut buffer = Vec::new();
        let mut reported = false;
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let line = String::from_utf8_lossy(&buffer);
            let line = line.trim_end_matches(['\r', '\n']);
            append_log(&log_path, line);
            if reported {
                continue;
            }
            if let Some(port) = parse_listening(line) {
                reported = true;
                if let Some(events) = &events {
                    let _ = events.send(HostEvent::Ready { port });
                }
            }
        }
    });
}

/// `host: listening on http://127.0.0.1:51234 (0 device(s) paired)` → 51234.
pub fn parse_listening(line: &str) -> Option<u16> {
    let rest = line.split("listening on http://").nth(1)?;
    let authority = rest.split('/').next()?.split_whitespace().next()?;
    authority.rsplit(':').next()?.parse().ok()
}

fn append_log(path: &Path, line: &str) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

/// Where the sidecar lives: an env override for tests, the dev copy next to
/// the app binary, the bundled plain name, or the build output.
pub fn host_binary_path(exe_dir: &Path) -> Option<PathBuf> {
    if let Ok(path) = std::env::var("HN_CORE_HOST") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let triple = env!("HN_TARGET_TRIPLE");
    let candidates = [
        exe_dir.join(format!("hn-core-host-{triple}.exe")),
        exe_dir.join("hn-core-host.exe"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(format!("hn-core-host-{triple}.exe")),
    ];
    candidates.into_iter().find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    fn config(program: &str, args: &[&str]) -> HostConfig {
        HostConfig {
            binary: PathBuf::from(program),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            data_dir: std::env::temp_dir().join(format!("hn-host-test-{}", std::process::id())),
            log_path: std::env::temp_dir().join(format!(
                "hn-host-test-{}-{}.log",
                std::process::id(),
                args.join("-").replace(['/', '\\', ' ', ':', '(', ')'], "_")
            )),
        }
    }

    #[test]
    fn the_ready_line_yields_the_real_port() {
        assert_eq!(
            parse_listening("host: listening on http://127.0.0.1:51234 (0 device(s) paired)"),
            Some(51234)
        );
        assert_eq!(parse_listening("host: agent w1:p1 = fake-agent"), None);
        assert_eq!(parse_listening(""), None);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_dead_child_is_restarted_until_cancelled() {
        let (events, mut received) = mpsc::unbounded_channel();
        let shutdown = CancellationToken::new();
        let handle = tokio::spawn(supervise(
            config("cmd.exe", &["/c", "exit 0"]),
            events,
            shutdown.clone(),
        ));

        let mut starts = 0;
        let mut restarts = 0;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < deadline && (starts < 2 || restarts < 1) {
            match tokio::time::timeout_at(deadline, received.recv()).await {
                Ok(Some(HostEvent::Started { .. })) => starts += 1,
                Ok(Some(HostEvent::Restarting { .. })) => restarts += 1,
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }
        assert!(starts >= 2, "el hijo muerto debe reiniciarse");
        assert!(restarts >= 1);

        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(3), handle)
            .await
            .expect("el supervisor debe parar al cancelar")
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelling_kills_a_live_child() {
        let (events, mut received) = mpsc::unbounded_channel();
        let shutdown = CancellationToken::new();
        let handle = tokio::spawn(supervise(
            config("cmd.exe", &["/c", "ping -n 30 127.0.0.1 >nul"]),
            events,
            shutdown.clone(),
        ));

        let started = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(HostEvent::Started { .. }) = received.recv().await {
                    return true;
                }
            }
        })
        .await
        .expect("el hijo debe arrancar");
        assert!(started);

        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("cancelar debe matar al hijo y terminar el supervisor")
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_log_receives_the_child_output() {
        let config = config("cmd.exe", &["/c", "echo hola"]);
        let (events, _received) = mpsc::unbounded_channel();
        let shutdown = CancellationToken::new();
        let handle = tokio::spawn(supervise(config.clone(), events, shutdown.clone()));
        tokio::time::sleep(Duration::from_millis(600)).await;
        let logged = std::fs::read_to_string(&config.log_path).unwrap_or_default();
        shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(3), handle).await;
        let _ = std::fs::remove_file(&config.log_path);
        assert!(logged.contains("hola"), "log fue: {logged:?}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_child_stdout_line_is_readable() {
        use tokio::process::Command;
        let mut child = Command::new("cmd.exe")
            .args(["/c", "echo hola"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        let line = lines.next_line().await.unwrap();
        assert_eq!(line.as_deref(), Some("hola"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn readiness_is_read_from_stdout() {
        let (events, mut received) = mpsc::unbounded_channel();
        let shutdown = CancellationToken::new();
        let handle = tokio::spawn(supervise(
            config(
                "cmd.exe",
                &[
                    "/c",
                    "echo host: listening on http://127.0.0.1:54321 (0 device(s) paired)",
                ],
            ),
            events,
            shutdown.clone(),
        ));

        let port = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(HostEvent::Ready { port }) = received.recv().await {
                    return port;
                }
            }
        })
        .await
        .expect("debe llegar el evento Ready");
        assert_eq!(port, 54321);

        shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(3), handle).await;
    }

    #[test]
    fn the_binary_path_prefers_the_env_override() {
        let dir = std::env::temp_dir().join(format!("hn-host-bin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("host-falso.exe");
        std::fs::write(&fake, b"x").unwrap();
        std::env::set_var("HN_CORE_HOST", &fake);
        assert_eq!(host_binary_path(&dir), Some(fake.clone()));
        std::env::remove_var("HN_CORE_HOST");
        assert_ne!(
            host_binary_path(&dir),
            Some(fake.clone()),
            "sin override no debe devolver el binario de prueba"
        );
        let _ = std::fs::remove_file(&fake);
    }
}
