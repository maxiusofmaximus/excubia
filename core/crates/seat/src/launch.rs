//! Turn a profile into a concrete, isolated launch plan. The matrix decides
//! *what* each CLI needs; this module only renders it. No shell is involved:
//! the program and every argument are an explicit vector.
//!
//! Real-CLI handshakes are `[NV]`; the test adapter exercises the same
//! materialisation, process launch and MCP transport end to end.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::profile::{CliProfile, PromptDelivery, TurnMode};

mod render;
#[cfg(test)]
mod tests;

pub use render::claude_settings;
use render::{claude_mcp, codex_config, opencode_config, test_mcp};

pub struct LaunchInput<'a> {
    pub profile: &'a CliProfile,
    pub run_id: &'a str,
    pub agent_id: &'a str,
    pub conversation_id: &'a str,
    pub run_dir: &'a Path,
    pub role: &'a str,
    pub prompt: &'a str,
    pub tool_url: &'a str,
    pub mcp_command: &'a Path,
    pub session_id: &'a str,
    /// The hook surface of the host and the token of this run. The hook token
    /// is the only credential that reaches the approval routes.
    pub hook_url: &'a str,
    pub hook_token: &'a str,
    /// The `hn-uac-hook` binary the CLI runs before each shell command.
    pub hook_command: &'a Path,
}

/// A resident panel: one CLI process per seat that reads tasks from stdin
/// instead of one process per turn. Real CLIs are `[NV]` here; the simulated
/// CLI exercises the same materialisation, token and MCP transport.
pub struct ResidentInput<'a> {
    pub profile: &'a CliProfile,
    pub agent_id: &'a str,
    pub role_id: &'a str,
    pub seat_dir: &'a Path,
    pub role: &'a str,
    pub tool_url: &'a str,
    pub mcp_command: &'a Path,
}

#[derive(Clone, Debug)]
pub struct LaunchPlan {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub stdin: String,
    pub files: Vec<(PathBuf, Vec<u8>)>,
}

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("launch file I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not encode launch configuration: {0}")]
    Encode(String),
    #[error("unsupported CLI capability combination for {0}")]
    Unsupported(String),
    /// Claude skips a hook it cannot run, and then runs the action. The launch
    /// refuses instead.
    #[error("the approval hook is missing or not runnable: {0}")]
    HookMissing(PathBuf),
}

/// Whether the hook can be run. A missing or non-executable file is not a
/// hook: Claude would treat its failure as non-blocking.
pub fn hook_ready(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    metadata.is_file() && is_runnable(path, &metadata)
}

#[cfg(unix)]
fn is_runnable(_: &Path, metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_runnable(path: &Path, _: &std::fs::Metadata) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
}

impl LaunchPlan {
    /// Write every file (role, MCP declaration, settings) atomically. The run
    /// directory is created first; the token never appears in any file.
    pub fn materialise(&self) -> Result<(), LaunchError> {
        std::fs::create_dir_all(&self.cwd)?;
        for (path, bytes) in &self.files {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            hn_core_persist::write_atomic(path, bytes)?;
        }
        Ok(())
    }
}

pub fn prepare(input: &LaunchInput<'_>) -> Result<LaunchPlan, LaunchError> {
    if !input.profile.is_consistent() {
        return Err(LaunchError::Unsupported(
            input.profile.id.as_str().to_string(),
        ));
    }
    let mut files = Vec::new();
    let mut env = common_env(input);
    let mut args = Vec::new();
    let program = input.profile.command.clone();
    let mcp_command = input.mcp_command.to_string_lossy().to_string();

    match input.profile.prompt {
        PromptDelivery::SystemPromptFile => {
            if !hook_ready(input.hook_command) {
                return Err(LaunchError::HookMissing(input.hook_command.to_path_buf()));
            }
            let role = input.run_dir.join("role.md");
            files.push((role.clone(), input.role.as_bytes().to_vec()));
            let settings = input.run_dir.join("claude-settings.json");
            files.push((settings.clone(), claude_settings(input.hook_command)?));
            let mcp = input.run_dir.join("claude-mcp.json");
            files.push((mcp.clone(), claude_mcp(&mcp_command, input.tool_url)?));
            args.extend([
                "-p".to_string(),
                "--append-system-prompt-file".to_string(),
                role.to_string_lossy().to_string(),
                "--mcp-config".to_string(),
                mcp.to_string_lossy().to_string(),
                "--strict-mcp-config".to_string(),
                "--setting-sources".to_string(),
                "project".to_string(),
                "--settings".to_string(),
                settings.to_string_lossy().to_string(),
                "--restricted".to_string(),
            ]);
        }
        PromptDelivery::DeveloperInstructions => {
            let codex_home = input.run_dir.join("codex");
            let config = codex_home.join("config.toml");
            files.push((
                config,
                codex_config(&mcp_command, input.tool_url, input.role)?,
            ));
            env.push((
                "CODEX_HOME".to_string(),
                codex_home.to_string_lossy().to_string(),
            ));
            args.extend([
                "-p".to_string(),
                "alfred".to_string(),
                "exec".to_string(),
                "--skip-git-repo-check".to_string(),
                "--color".to_string(),
                "never".to_string(),
                "-s".to_string(),
                "read-only".to_string(),
                "-a".to_string(),
                "never".to_string(),
                "--disable".to_string(),
                "shell_tool".to_string(),
            ]);
        }
        PromptDelivery::AgentConfig => {
            let config_dir = input.run_dir.join("opencode");
            let config = config_dir.join("opencode.json");
            files.push((
                config,
                opencode_config(&mcp_command, input.tool_url, input.role)?,
            ));
            env.extend([
                (
                    "OPENCODE_CONFIG_DIR".to_string(),
                    config_dir.to_string_lossy().to_string(),
                ),
                (
                    "OPENCODE_DISABLE_PROJECT_CONFIG".to_string(),
                    "1".to_string(),
                ),
            ]);
            args.extend([
                "run".to_string(),
                "--standalone".to_string(),
                "--agent".to_string(),
                "alfred".to_string(),
            ]);
            if input.profile.mode == TurnMode::Session {
                args.extend(["--session".to_string(), input.session_id.to_string()]);
            }
        }
        PromptDelivery::TestStdin => {
            let mcp = input.run_dir.join("mcp.json");
            files.push((mcp.clone(), test_mcp(&mcp_command, input.tool_url)?));
            args.extend([
                "--mcp-config".to_string(),
                mcp.to_string_lossy().to_string(),
            ]);
        }
    }

    Ok(LaunchPlan {
        program,
        args,
        env,
        cwd: input.run_dir.to_path_buf(),
        stdin: input.prompt.to_string(),
        files,
    })
}

/// Build a resident panel plan. The test adapter is the only prompt delivery
/// that defines resident mode today; the shipped CLIs need their own resident
/// handshake, which is `[NV]` (no quota is spent in this milestone).
pub fn prepare_resident(input: &ResidentInput<'_>) -> Result<LaunchPlan, LaunchError> {
    if !input.profile.is_consistent() {
        return Err(LaunchError::Unsupported(
            input.profile.id.as_str().to_string(),
        ));
    }
    if input.profile.prompt != PromptDelivery::TestStdin {
        return Err(LaunchError::Unsupported(format!(
            "{} has no resident panel mode yet",
            input.profile.id.as_str()
        )));
    }
    let role_file = input.seat_dir.join("role.md");
    let mcp = input.seat_dir.join("mcp.json");
    let mcp_command = input.mcp_command.to_string_lossy().to_string();
    Ok(LaunchPlan {
        program: input.profile.command.clone(),
        args: vec![
            "--resident".to_string(),
            "--mcp-config".to_string(),
            mcp.to_string_lossy().to_string(),
        ],
        env: vec![
            ("HN_ENV".to_string(), "1".to_string()),
            ("HN_CHAT_AGENT_ID".to_string(), input.agent_id.to_string()),
            ("HN_SEAT_ROLE".to_string(), input.role_id.to_string()),
            ("HN_TOOL_URL".to_string(), input.tool_url.to_string()),
        ],
        cwd: input.seat_dir.to_path_buf(),
        stdin: String::new(),
        files: vec![
            (role_file, input.role.as_bytes().to_vec()),
            (mcp, test_mcp(&mcp_command, input.tool_url)?),
        ],
    })
}

fn common_env(input: &LaunchInput<'_>) -> Vec<(String, String)> {
    let mut env = vec![
        ("HN_ENV".to_string(), "1".to_string()),
        ("HN_CHAT_RUN_ID".to_string(), input.run_id.to_string()),
        // The OpenCode plugin reads `HN_RUN_ID`; keep both spellings of the
        // same fact so no adapter has to guess.
        ("HN_RUN_ID".to_string(), input.run_id.to_string()),
        ("HN_CHAT_AGENT_ID".to_string(), input.agent_id.to_string()),
        (
            "HN_CHAT_CONVERSATION_ID".to_string(),
            input.conversation_id.to_string(),
        ),
        ("HN_TOOL_URL".to_string(), input.tool_url.to_string()),
    ];
    if !input.hook_url.is_empty() {
        env.push(("HN_HOOK_URL".to_string(), input.hook_url.to_string()));
        env.push(("HN_HOOK_TOKEN".to_string(), input.hook_token.to_string()));
    }
    env
}
