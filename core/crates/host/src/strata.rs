//! The Strata local backend behind `excubia-host strata`.
//!
//! Strata serves one local model on loopback (`strata launch`). The host starts
//! it on demand, waits until it answers, and stops it. One thing at a time: the
//! model needs most of the machine's memory, so `start` waits until the voice
//! host has let go of its ASR (`local_mode.rs`). The contract is
//! `docs/contracts/local-backend.md`.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use excubia_api::AgentCatalog;
use serde_json::Value;

use crate::cli::{StrataCommand, BIN};
use crate::{agents, alfred_backend, local_mode, paths};

/// The one model the local server loads.
pub const MODEL: &str = "qwen3.8-flash-next-q2_0";
const PORT: u16 = 8080;
const GIB: u64 = 1 << 30;
/// `start` refuses below this much free memory: the model takes ~34 GB.
const MIN_FREE_RAM: u64 = 38 * GIB;
/// A cold load took ~236 s on this machine; the wait is capped here.
const LOAD_TIMEOUT: Duration = Duration::from_secs(600);
const BACKOFF_START: Duration = Duration::from_secs(2);
const BACKOFF_CAP: Duration = Duration::from_secs(15);
/// `start` waits this long for the voice host to let go of its memory.
const RAM_TIMEOUT: Duration = Duration::from_secs(60);
const AGENT_NAME: &str = "claude-local";
/// The only variables a Strata child receives. `STRATA_API_KEY` is not here:
/// when it is set, Strata demands it and answers 401.
const CHILD_ENV: &[&str] = &[
    "PATH",
    "PATHEXT",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "COMSPEC",
    "USERPROFILE",
    "USERNAME",
    "COMPUTERNAME",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "TEMP",
    "TMP",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
    "CUDA_PATH",
];
const LAUNCH_ARGS: &[&str] = &[
    "launch",
    "--model",
    "q2_0",
    "--context",
    "64k",
    "--vision",
    "off",
    "--parallel",
    "1",
    "--tune",
    "skip",
    "--access",
    "local",
    "--port",
    "8080",
    "--no-api-key",
    "--no-tui",
    "--yes",
];

/// Where the local server stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Stopped,
    /// Launched, but `/health` does not report the model loaded yet.
    Loading,
    Ready,
}

pub fn endpoint() -> String {
    format!("http://127.0.0.1:{PORT}")
}

/// The launcher: `STRATA_BIN`, else the install's default path.
pub(crate) fn binary() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("STRATA_BIN") {
        return Ok(PathBuf::from(path));
    }
    let local = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| anyhow!("LOCALAPPDATA is not set: set STRATA_BIN to strata.cmd"))?;
    Ok(PathBuf::from(local)
        .join("Strata")
        .join("terminal")
        .join("0.1.47")
        .join("bin")
        .join("strata.cmd"))
}

/// A `strata` invocation whose environment is built from the allowlist alone.
fn strata_command(bin: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(bin);
    command
        .args(args)
        .env_clear()
        .envs(
            CHILD_ENV
                .iter()
                .filter_map(|&key| std::env::var_os(key).map(|value| (OsString::from(key), value))),
        )
        .stdin(Stdio::null());
    command
}

/// `GET /health` on loopback: the body when it answers 200, `None` when
/// nothing does (stopped, or still loading).
fn health() -> Option<Value> {
    let address = SocketAddr::from(([127, 0, 0, 1], PORT));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    parse_health(&response)
}

/// The body of a `/health` answer. Strata's server is Python's BaseHTTP, which
/// answers `HTTP/1.0 200 OK`, so any `HTTP/1.x` version counts.
fn parse_health(response: &str) -> Option<Value> {
    let (head, body) = response.split_once("\r\n\r\n")?;
    let mut status = head.lines().next()?.split_whitespace();
    if !status.next()?.starts_with("HTTP/1.") || status.next()? != "200" {
        return None;
    }
    let json = body.get(body.find('{')?..=body.rfind('}')?)?;
    serde_json::from_str(json).ok()
}

/// The entries `strata launch --status --output json` lists as running. The
/// JSON is the first line that starts with `{`; Strata prints a text line after it.
fn launch_running(output: &str) -> Result<Vec<Value>> {
    let line = output
        .lines()
        .find(|line| line.starts_with('{'))
        .context("strata's status has no JSON line")?;
    let report: Value = serde_json::from_str(line).context("strata's status line is not JSON")?;
    Ok(report["running"].as_array().cloned().unwrap_or_default())
}

/// Where the server stands. `/health` answers only once the model is loaded;
/// `launch --status` tells a loading server from a stopped one. Any running
/// entry means Strata is still starting: `/health` alone says "ready".
pub fn state(bin: &Path) -> Result<State> {
    if let Some(body) = health() {
        return Ok(if body["loaded"] == true {
            State::Ready
        } else {
            State::Loading
        });
    }
    let output = strata_command(bin, &["launch", "--status", "--output", "json"])
        .output()
        .context("ask strata for its status")?;
    let running = launch_running(&String::from_utf8_lossy(&output.stdout))?;
    Ok(if running.is_empty() {
        State::Stopped
    } else {
        State::Loading
    })
}

/// Free physical memory, in bytes.
#[cfg(windows)]
fn free_ram() -> Result<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: MEMORYSTATUSEX holds only integers, so all-zero is a valid value.
    let mut memory: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    memory.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: `memory` is a valid, writable MEMORYSTATUSEX sized in dwLength.
    if unsafe { GlobalMemoryStatusEx(&mut memory) } == 0 {
        bail!("could not read the free memory");
    }
    Ok(memory.ullAvailPhys)
}

#[cfg(not(windows))]
fn free_ram() -> Result<u64> {
    bail!("the memory check only runs on Windows")
}

fn ram_ok(free: u64) -> Result<()> {
    if free < MIN_FREE_RAM {
        bail!(
            "hay {:.1} GiB libres; Strata necesita al menos {} GiB",
            free as f64 / GIB as f64,
            MIN_FREE_RAM / GIB
        );
    }
    Ok(())
}

/// Waits until the memory is free: the voice host drops its ASR when it sees the marker,
/// and that takes a moment. Gives up after `RAM_TIMEOUT`.
fn wait_for_free_ram() -> Result<()> {
    let started = Instant::now();
    let mut delay = BACKOFF_START;
    loop {
        let free = free_ram()?;
        if ram_ok(free).is_ok() {
            return Ok(());
        }
        if started.elapsed() >= RAM_TIMEOUT {
            return ram_ok(free).context("the voice host did not let go of its memory");
        }
        println!(
            "esperando memoria: {:.1} GiB libres",
            free as f64 / GIB as f64
        );
        std::thread::sleep(delay);
        delay = (delay * 2).min(BACKOFF_CAP);
    }
}

/// Whether a model server is up or starting. The daemon asks once, at its startup.
pub fn is_running() -> bool {
    match binary().and_then(|bin| state(&bin)) {
        Ok(state) => state != State::Stopped,
        Err(_) => health().is_some(),
    }
}

/// Poll `/health` with a growing wait until the model loads, the launch fails
/// or the cap passes.
fn wait_until_loaded(bin: &Path, mut launcher: Option<Child>) -> Result<()> {
    let started = Instant::now();
    let mut delay = BACKOFF_START;
    let loaded = || health().is_some_and(|body| body["loaded"] == true);
    loop {
        if loaded() {
            println!("listo en {} s: {}", started.elapsed().as_secs(), endpoint());
            return Ok(());
        }
        let exit = match launcher.as_mut() {
            Some(child) => child.try_wait().context("watch strata launch")?,
            None => None,
        };
        if let Some(status) = exit {
            if !status.success() {
                bail!("strata launch terminó con {status}");
            }
            // A clean exit is not the verdict: the server may already serve.
            launcher = None;
            if loaded() {
                println!("listo en {} s: {}", started.elapsed().as_secs(), endpoint());
                return Ok(());
            }
            if state(bin)? == State::Stopped {
                bail!("strata launch terminó y no queda nada en marcha");
            }
        }
        let elapsed = started.elapsed();
        if elapsed >= LOAD_TIMEOUT {
            bail!(
                "no cargó en {} s; `{BIN} strata status` dice cómo está",
                LOAD_TIMEOUT.as_secs()
            );
        }
        println!("cargando... {} s", elapsed.as_secs());
        std::thread::sleep(delay);
        delay = (delay * 2).min(BACKOFF_CAP);
    }
}

/// Marks the voice host to let go of its ASR, then launches the model. A failed or expired
/// start removes the marker again, so the voice host comes back on its own.
pub fn start(bin: &Path, data_dir: &Path) -> Result<()> {
    local_mode::set(data_dir).context("write the local-mode marker")?;
    let outcome = start_model(bin);
    if outcome.is_err() {
        if let Err(error) = clear_markers(data_dir) {
            eprintln!("host: could not remove a model marker: {error:#}");
        }
    }
    outcome
}

fn start_model(bin: &Path) -> Result<()> {
    match state(bin)? {
        State::Ready => println!("ya está listo: {}", endpoint()),
        State::Loading => {
            println!("ya está cargando; espero");
            wait_until_loaded(bin, None)?;
        }
        State::Stopped => {
            wait_for_free_ram()?;
            println!("lanzando Strata con {MODEL}");
            let child = strata_command(bin, LAUNCH_ARGS)
                .spawn()
                .context("start strata launch")?;
            wait_until_loaded(bin, Some(child))?;
        }
    }
    Ok(())
}

fn clear_markers(data_dir: &Path) -> Result<()> {
    local_mode::clear(data_dir).context("remove the local-mode marker")?;
    alfred_backend::clear(data_dir).context("remove the alfred-local marker")
}

/// Stops the model, then removes the markers. A failed stop keeps them: the model may run.
pub fn stop(bin: &Path, data_dir: &Path) -> Result<()> {
    if state(bin)? != State::Stopped {
        let status = strata_command(bin, &["launch", "--stop"])
            .status()
            .context("run strata launch --stop")?;
        if !status.success() {
            bail!("strata launch --stop terminó con {status}");
        }
    }
    clear_markers(data_dir)?;
    println!("parado");
    Ok(())
}

fn status(bin: &Path, data_dir: &Path) -> Result<()> {
    match state(bin)? {
        State::Stopped => println!("parado"),
        State::Loading => println!("cargando"),
        State::Ready => println!("listo ({})", endpoint()),
    }
    let on = local_mode::is_on(data_dir);
    println!("modo local: {}", if on { "sí" } else { "no" });
    Ok(())
}

fn agent_block() -> String {
    let env: Vec<String> = alfred_backend::local_env()
        .iter()
        .map(|(key, value)| format!("{key} = \"{value}\""))
        .collect();
    format!(
        "\n[agents.{AGENT_NAME}]\ncommand = \"claude\"\nargs = []\n\
         label = \"Claude Code (local Strata)\"\nenv = {{ {} }}\n",
        env.join(", ")
    )
}

/// Write `[agents.claude-local]` into `agents.toml` unless it is there already.
/// Returns whether it wrote the entry.
pub fn enable_agent(data_dir: &Path) -> Result<bool> {
    let path = paths::agents_path(data_dir);
    let catalog = agents::load_or_create(data_dir)?;
    if catalog.names().iter().any(|name| name == AGENT_NAME) {
        return Ok(false);
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let text = format!("{text}{}", agent_block());
    // Refuse a bad entry before writing it: the file would not load.
    AgentCatalog::from_toml(&text).map_err(|error| anyhow!("{error}"))?;
    excubia_persist::write_atomic(&path, text.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;
    Ok(true)
}

/// Run one `excubia-host strata` subcommand.
pub fn run(command: StrataCommand, data_dir: &Path) -> Result<()> {
    match command {
        StrataCommand::EnableAgent => {
            if enable_agent(data_dir)? {
                println!(
                    "entrada `{AGENT_NAME}` escrita en {}",
                    paths::agents_path(data_dir).display()
                );
            } else {
                println!("`{AGENT_NAME}` ya existe; no lo toco");
            }
            Ok(())
        }
        StrataCommand::Start => start(&binary()?, data_dir),
        StrataCommand::Stop => stop(&binary()?, data_dir),
        StrataCommand::Status => status(&binary()?, data_dir),
    }
}

#[cfg(test)]
mod tests;
