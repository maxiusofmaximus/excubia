//! Shared harness for the real-host integration tests.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use herdr_notify_desktop::host_process::{supervise, HostConfig, HostEvent};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio_util::sync::CancellationToken;

pub fn host_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("HN_CORE_HOST") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let triple = env!("HN_TARGET_TRIPLE");
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    [
        manifest
            .join("binaries")
            .join(format!("hn-core-host-{triple}.exe")),
        manifest
            .join("..")
            .join("target")
            .join("debug")
            .join(format!("hn-core-host-{triple}.exe")),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

pub fn fake_agent_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("HN_FAKE_AGENT") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let desktop = manifest.parent()?;
    [
        desktop.join("target").join("fake-agent.exe"),
        desktop
            .join("..")
            .join("core")
            .join("target")
            .join("debug")
            .join("fake-agent.exe"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

pub fn run_cli(binary: &Path, data_dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new(binary)
        .args(args)
        .arg("--data-dir")
        .arg(data_dir)
        .output()
        .expect("el CLI del host debe ejecutarse");
    assert!(
        output.status.success(),
        "host {:?} falló: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub fn daemon_args(data_dir: &Path, spawns: &[String]) -> Vec<String> {
    let mut args = vec![
        "--port".to_string(),
        "0".to_string(),
        "--label".to_string(),
        "integration".to_string(),
        "--data-dir".to_string(),
        data_dir.display().to_string(),
    ];
    for spawn in spawns {
        args.push("--spawn".to_string());
        args.push(spawn.clone());
    }
    args
}

pub struct RunningHost {
    pub port: u16,
    pub pid: u32,
    pub binary: PathBuf,
    pub data_dir: PathBuf,
    pub events: UnboundedReceiver<HostEvent>,
    shutdown: CancellationToken,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl RunningHost {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub async fn next_ready(&mut self) -> (u16, u32) {
        let (port, pid) = next_ready(&mut self.events).await;
        self.port = port;
        self.pid = pid;
        (port, pid)
    }

    pub async fn stop(mut self) {
        self.shutdown.cancel();
        if let Some(task) = self.task.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
        }
    }
}

/// Starts the real daemon. Returns `None` (a skipped test) when its binaries
/// have not been built.
pub async fn start_host(data_dir: PathBuf, spawns: Vec<String>) -> Option<RunningHost> {
    let binary = host_binary()?;
    std::fs::create_dir_all(&data_dir).ok()?;
    let log_path = data_dir.join("host.log");
    let (events, mut received) = mpsc::unbounded_channel();
    let shutdown = CancellationToken::new();
    let task = tokio::spawn(supervise(
        HostConfig {
            binary: binary.clone(),
            args: daemon_args(&data_dir, &spawns),
            data_dir: data_dir.clone(),
            log_path,
        },
        events,
        shutdown.clone(),
    ));
    let (port, pid) = next_ready(&mut received).await;
    Some(RunningHost {
        port,
        pid,
        binary,
        data_dir,
        events: received,
        shutdown,
        task: Some(task),
    })
}

/// Waits for readiness, remembering the pid of the child that became ready.
pub async fn next_ready(received: &mut UnboundedReceiver<HostEvent>) -> (u16, u32) {
    let mut pid = 0;
    loop {
        match tokio::time::timeout(Duration::from_secs(30), received.recv()).await {
            Ok(Some(HostEvent::Started { pid: started })) => pid = started,
            Ok(Some(HostEvent::Ready { port })) => return (port, pid),
            Ok(Some(_)) => {}
            Ok(None) => panic!("el supervisor terminó antes de estar listo"),
            Err(_) => panic!("timeout esperando al host"),
        }
    }
}
