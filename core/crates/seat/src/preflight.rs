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
use crate::profile::{CliId, CliProfile};

/// The line OpenCode writes when a plugin does not load. The plugin cannot
/// ask for an approval it never loaded, so this is a hard refusal.
pub const LOAD_FAILURE_MARKER: &str = "failed to load plugin";
/// The prefix of the denial the plugin throws when it blocks a tool.
pub const BLOCKED_MARKER: &str = "[UAC]";
/// The canary command the plugin must block. It leaves a file behind when it
/// runs, so the proof is not only a log line.
pub const CANARY_COMMAND: &str = "echo hn-uac-canary > hn-uac-canary.txt";
/// The file [`CANARY_COMMAND`] would create.
pub const CANARY_MARKER: &str = "hn-uac-canary.txt";

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
        .env_remove("HN_HOOK_URL")
        .env_remove("HN_HOOK_TOKEN");
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
    let provider = match CanaryProvider::start(CANARY_COMMAND).await {
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
    let config = work_dir.join("opencode.json");
    write_config(&config, plugin_dir, &base_url);
    let env = canary_env(&config, cache_dir);
    CanaryPlan {
        program: profile.command.clone(),
        args: vec![
            "run".to_string(),
            "--print-logs".to_string(),
            "--log-level".to_string(),
            "INFO".to_string(),
            "--format".to_string(),
            "json".to_string(),
            "--model".to_string(),
            "mock/mock".to_string(),
            "--agent".to_string(),
            "build".to_string(),
            "--dir".to_string(),
            path(work_dir),
            format!("Ejecuta este comando de prueba: {CANARY_COMMAND}"),
        ],
        env,
        cwd: work_dir.to_path_buf(),
        marker: work_dir.join(CANARY_MARKER),
        provider,
    }
}

/// The isolated OpenCode configuration of the canary: only the plugin and the
/// local provider, so it never reads the operator's settings or quota.
fn write_config(config: &Path, plugin_dir: &Path, base_url: &str) {
    // The CLI resolves a relative plugin path against its own directory and
    // rejects Windows verbatim paths (`\\?\`), so the canary hands it a plain
    // absolute one.
    let plugin = std::path::absolute(plugin_dir).unwrap_or_else(|_| plugin_dir.to_path_buf());
    let body = serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "autoupdate": false,
        "share": "disabled",
        "plugin": [plugin.to_string_lossy()],
        "permission": { "bash": "allow", "edit": "allow", "read": "allow", "webfetch": "allow" },
        "provider": {
            "mock": {
                "npm": "@ai-sdk/openai-compatible",
                "name": "Mock",
                "options": { "baseURL": base_url, "apiKey": "dummy" },
                "models": {
                    "mock": {
                        "name": "Mock",
                        "tool_call": true,
                        "limit": { "context": 200000, "output": 8192 }
                    }
                }
            }
        },
        "model": "mock/mock"
    });
    match serde_json::to_vec_pretty(&body) {
        Ok(bytes) => {
            let _ = std::fs::write(config, bytes);
        }
        Err(error) => eprintln!("seat: the UAC canary config was not written: {error}"),
    }
}

/// The environment of the canary. `cache_dir` is shared by every seat of the
/// host: OpenCode installs its plugin SDK in `xdg/config/opencode` on first
/// use, so only the first canary pays for it (or nothing, when the host
/// preinstalled that tree with pnpm).
fn canary_env(config: &Path, cache_dir: &Path) -> Vec<(String, String)> {
    let home = cache_dir.join("home");
    let _ = std::fs::create_dir_all(&home);
    let values = [
        ("OPENCODE_CONFIG", config.to_path_buf()),
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
    use hn_core_test_dir::TestDir;

    #[test]
    fn a_plugin_that_did_not_load_is_unsafe() {
        let outcome = classify("failed to load plugin: herdr.uac");
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
        let config = dir.join("opencode.json");

        write_config(&config, Path::new("tooling"), "http://127.0.0.1:9/v1");

        let body: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&config).expect("config")).expect("json");
        let plugin = body["plugin"][0].as_str().expect("plugin path");
        assert!(Path::new(plugin).is_absolute(), "{plugin}");
        assert!(!plugin.starts_with(r"\\?\"), "verbatim path: {plugin}");
        assert_eq!(body["provider"]["mock"]["npm"], "@ai-sdk/openai-compatible");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
