//! The UAC preflight: the canary that proves a blocking adapter works.
//!
//! `docs/rfc/orchestrator-chat.md` §5: OpenCode's UAC only counts when the
//! host verified that the plugin loaded *and* blocked. The verification is a
//! canary: the CLI is started with the plugin and a test command, and the host
//! reads three facts — the plugin loaded, the command was blocked, and the
//! command did not run. Anything else is `unsafe` and the seat is never
//! launched.
//!
//! The canary costs nothing: the OpenCode adapter runs against the in-process
//! provider of [`crate::canary_provider`], the Rust twin of the provider the
//! `e2e/` harness of the plugin already uses. No model quota is spent.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

use crate::canary_provider::CanaryProvider;
use crate::launch::{opencode_config, OpenCodeRun};
use crate::profile::{CliId, CliProfile};

/// The line OpenCode writes when a plugin does not load. The plugin cannot
/// ask for an approval it never loaded, so this is a hard refusal.
pub const LOAD_FAILURE_MARKER: &str = "failed to load plugin";
/// The prefix of the denial the plugin throws when it blocks a tool.
pub const BLOCKED_MARKER: &str = "[UAC]";
/// The file the canary command writes when it runs. The name is relative to the
/// canary's work directory; the command gets the absolute path.
pub const CANARY_MARKER: &str = "excubia-uac-canary.txt";

/// The canary command the plugin must block. It leaves `marker` behind when it
/// runs, so the proof is not only a log line. The path must be absolute: the CLI
/// runs shell commands in its own project directory, not in the spawn's, so a
/// relative marker lands elsewhere and a "no marker" verdict proves nothing.
pub fn canary_command(marker: &Path) -> String {
    format!("echo excubia-uac-canary > \"{}\"", marker.to_string_lossy())
}

/// The verdict of the preflight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UacState {
    /// No adapter was checked: the seat is degraded but launchable.
    Unchecked,
    /// The adapter loaded and blocked: the seat is safe.
    Safe,
    /// The adapter did not load or did not block: the seat is not launched.
    Unsafe,
}

impl UacState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unchecked => "unchecked",
            Self::Safe => "safe",
            Self::Unsafe => "unsafe",
        }
    }
}

/// What the canary runs. It owns the local provider, so the plan cannot
/// outlive the server the CLI is meant to talk to.
pub struct CanaryPlan {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    /// The file the test command would create if the plugin let it run.
    pub marker: PathBuf,
    provider: Option<CanaryProvider>,
}

impl CanaryPlan {
    /// The base URL of the local provider, for the diagnostics of a run.
    pub fn provider_url(&self) -> Option<&str> {
        self.provider.as_ref().map(CanaryProvider::base_url)
    }
}

/// The verdict and the reason behind it.
#[derive(Clone, Debug)]
pub struct CanaryOutcome {
    pub state: UacState,
    pub reason: String,
}

impl CanaryOutcome {
    fn of(state: UacState, reason: &str) -> Self {
        Self {
            state,
            reason: reason.to_string(),
        }
    }
}

/// Run the canary and read its verdict. A canary that cannot run, that hangs,
/// that answers nothing or that lets the command run is `unsafe`: the seat is
/// not launched.
pub async fn run_canary(plan: &CanaryPlan, limit: Duration) -> CanaryOutcome {
    // A marker from a previous run would fake the proof of this one.
    let _ = std::fs::remove_file(&plan.marker);
    let mut command = Command::new(&plan.program);
    command
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (key, value) in &plan.env {
        command.env(key, value);
    }
    // Without a host the plugin denies at its first risky call: the canary
    // proves the block, it never needs an operator.
    command
        .env_remove("EXCUBIA_HOOK_URL")
        .env_remove("EXCUBIA_HOOK_TOKEN");
    let output = match timeout(limit, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return CanaryOutcome::of(
                UacState::Unsafe,
                &format!("the UAC canary could not run: {error}"),
            )
        }
        Err(_) => {
            return CanaryOutcome::of(UacState::Unsafe, "the UAC canary did not answer in time")
        }
    };
    let spoken = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if plan.marker.exists() {
        return CanaryOutcome::of(
            UacState::Unsafe,
            "the canary command ran: the UAC adapter did not block it",
        );
    }
    classify(&spoken)
}

/// Read the verdict of a canary that already spoke.
pub fn classify(spoken: &str) -> CanaryOutcome {
    if spoken.contains(LOAD_FAILURE_MARKER) {
        return CanaryOutcome::of(UacState::Unsafe, "the UAC plugin did not load");
    }
    if spoken.contains(BLOCKED_MARKER) {
        return CanaryOutcome::of(
            UacState::Safe,
            "the UAC plugin loaded and blocked the canary",
        );
    }
    CanaryOutcome::of(
        UacState::Unsafe,
        "the canary was not blocked: the UAC adapter is not proven",
    )
}

/// The canary of one CLI, or `None` when the CLI has no blocking adapter.
///
/// `cache_dir` is shared by every seat of the host: OpenCode installs its
/// plugin SDK there on first use, so only the first canary pays for it. The
/// host can preinstall that tree with `tooling/seed-uac-canary.ps1` to make
/// the first pass offline.
pub async fn canary_plan(
    profile: &CliProfile,
    plugin_dir: &Path,
    work_dir: &Path,
    cache_dir: &Path,
) -> Option<CanaryPlan> {
    match profile.id {
        CliId::OpenCode => Some(opencode_canary(profile, plugin_dir, work_dir, cache_dir).await),
        CliId::Fake => Some(fake_canary(profile, plugin_dir, work_dir)),
        _ => None,
    }
}

/// The OpenCode canary: an isolated configuration that declares the plugin and
/// a local provider, so asking the "model" for the test command is free.
async fn opencode_canary(
    profile: &CliProfile,
    plugin_dir: &Path,
    work_dir: &Path,
    cache_dir: &Path,
) -> CanaryPlan {
    let marker = std::path::absolute(work_dir.join(CANARY_MARKER))
        .unwrap_or_else(|_| work_dir.join(CANARY_MARKER));
    let command = canary_command(&marker);
    let provider = match CanaryProvider::start(&command).await {
        Ok(provider) => Some(provider),
        Err(error) => {
            eprintln!("seat: the UAC canary provider did not start: {error}");
            None
        }
    };
    let base_url = provider
        .as_ref()
        .map(CanaryProvider::base_url)
        .unwrap_or("http://127.0.0.1:9/v1")
        .to_string();
    // The seat's own config directory, so the canary reads what a seat reads.
    let config_dir = work_dir.join("opencode");
    write_config(&config_dir, plugin_dir, &base_url);
    let env = canary_env(&config_dir, cache_dir);
    CanaryPlan {
        program: profile.command.clone(),
        // The working directory is the spawn's `current_dir`: opencode 2.2.6 has
        // no `--dir` flag, and it takes `--log-level` in lowercase only.
        args: vec![
            "run".to_string(),
            "--standalone".to_string(),
            "--print-logs".to_string(),
            "--log-level".to_string(),
            "info".to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--model".to_string(),
            "mock/mock".to_string(),
            "--agent".to_string(),
            "alfred".to_string(),
            format!("Ejecuta este comando de prueba: {command}"),
        ],
        env,
        cwd: work_dir.to_path_buf(),
        marker,
        provider,
    }
}

/// Write the canary's configuration into `config_dir`. The canary and a seat
/// render through the same generator, so the canary proves the seat's blocking
/// setup. A write that fails leaves no configuration: the plugin does not load
/// and the canary is `unsafe`.
fn write_config(config_dir: &Path, plugin_dir: &Path, base_url: &str) {
    let _ = std::fs::create_dir_all(config_dir);
    let run = OpenCodeRun::Canary {
        provider_url: base_url,
    };
    match opencode_config(Some(plugin_dir), run) {
        Ok(bytes) => {
            if let Err(error) = std::fs::write(config_dir.join("opencode.json"), bytes) {
                eprintln!("seat: the UAC canary config was not written: {error}");
            }
        }
        Err(error) => eprintln!("seat: the UAC canary config was not encoded: {error}"),
    }
}

/// The environment of the canary. `cache_dir` is shared by every seat of the
/// host: OpenCode installs its plugin SDK in `xdg/config/opencode` on first
/// use, so only the first canary pays for it (or nothing, when the host
/// preinstalled that tree with pnpm).
fn canary_env(config_dir: &Path, cache_dir: &Path) -> Vec<(String, String)> {
    let home = cache_dir.join("home");
    let _ = std::fs::create_dir_all(&home);
    let values = [
        ("OPENCODE_CONFIG_DIR", config_dir.to_path_buf()),
        ("OPENCODE_DISABLE_PROJECT_CONFIG", PathBuf::from("1")),
        ("HOME", home.clone()),
        ("USERPROFILE", home),
        ("XDG_CONFIG_HOME", cache_dir.join("xdg").join("config")),
        ("XDG_DATA_HOME", cache_dir.join("xdg").join("data")),
        ("XDG_CACHE_HOME", cache_dir.join("xdg").join("cache")),
        ("XDG_STATE_HOME", cache_dir.join("xdg").join("state")),
    ];
    values
        .iter()
        .map(|(key, value)| (key.to_string(), path(value)))
        .collect()
}

/// The simulated canary: the CLI loads the plugin the way OpenCode does and
/// the plugin blocks the test command.
fn fake_canary(profile: &CliProfile, plugin_dir: &Path, work_dir: &Path) -> CanaryPlan {
    CanaryPlan {
        program: profile.command.clone(),
        args: vec![
            "--uac-canary".to_string(),
            "--uac-plugin".to_string(),
            path(plugin_dir),
        ],
        env: Vec::new(),
        cwd: work_dir.to_path_buf(),
        marker: work_dir.join(CANARY_MARKER),
        provider: None,
    }
}

fn path(value: &Path) -> String {
    value.to_string_lossy().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use excubia_test_dir::TestDir;

    #[test]
    fn a_plugin_that_did_not_load_is_unsafe() {
        let outcome = classify("failed to load plugin: excubia.uac");
        assert_eq!(outcome.state, UacState::Unsafe);
        assert!(outcome.reason.contains("did not load"), "{outcome:?}");
    }

    #[test]
    fn a_blocked_canary_is_safe() {
        let outcome = classify("[UAC] aprobación denegada por el operador");
        assert_eq!(outcome.state, UacState::Safe);
    }

    #[test]
    fn a_silent_canary_is_unsafe() {
        let outcome = classify("everything went fine");
        assert_eq!(outcome.state, UacState::Unsafe);
    }

    #[test]
    fn the_load_failure_wins_over_the_block() {
        let outcome = classify("failed to load plugin\n[UAC] denegada");
        assert_eq!(outcome.state, UacState::Unsafe);
    }

    #[test]
    fn the_canary_config_uses_a_plain_absolute_plugin_path() {
        let dir = TestDir::new("canary-config");
        let config_dir = dir.join("opencode");

        write_config(&config_dir, Path::new("tooling"), "http://127.0.0.1:9/v1");

        let body: serde_json::Value = serde_json::from_slice(
            &std::fs::read(config_dir.join("opencode.json")).expect("config"),
        )
        .expect("json");
        let plugin = body["plugin"][0].as_str().expect("plugin path");
        assert!(Path::new(plugin).is_absolute(), "{plugin}");
        assert!(!plugin.starts_with(r"\\?\"), "verbatim path: {plugin}");
        assert_eq!(body["provider"]["mock"]["npm"], "@ai-sdk/openai-compatible");
    }

    #[test]
    fn the_canary_and_a_seat_share_the_permission_block_and_the_plugin_key() {
        let canary = opencode_config(
            Some(Path::new("tooling")),
            OpenCodeRun::Canary {
                provider_url: "http://127.0.0.1:9/v1",
            },
        )
        .expect("canary");
        let seat = opencode_config(
            Some(Path::new("tooling")),
            OpenCodeRun::Seat {
                command: "excubia-mcp",
                tool_url: "http://127.0.0.1:9/internal/chat/tools",
                role: "role",
            },
        )
        .expect("seat");
        let canary: serde_json::Value = serde_json::from_slice(&canary).expect("json");
        let seat: serde_json::Value = serde_json::from_slice(&seat).expect("json");

        assert_eq!(canary["permission"], seat["permission"]);
        assert_eq!(canary["plugin"], seat["plugin"]);
        assert_eq!(
            seat["permission"],
            // With the plugin registered, shell runs and the plugin is the gate.
            serde_json::json!({ "bash": "allow", "edit": "deny", "webfetch": "deny" })
        );
        assert!(canary.get("mcp").is_none(), "the canary has no MCP server");
        assert_eq!(seat["mcp"]["excubia-core"]["type"], "local");
        assert_eq!(seat["agent"]["alfred"]["prompt"], "role");
    }
}
