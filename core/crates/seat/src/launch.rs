//! Turn a profile into a concrete, isolated launch plan. The matrix decides
//! *what* each CLI needs; this module only renders it. No shell is involved:
//! the program and every argument are an explicit vector.
//!
//! Real-CLI handshakes are `[NV]`; the test adapter exercises the same
//! materialisation, process launch and MCP transport end to end.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::profile::{CliProfile, PromptDelivery};

/// The OpenCode model Alfred's engine turns run on (free tier, the Master's pick: DeepSeek V4.1 Flash is better than GLM 5.3 Flash).
const OPENCODE_ENGINE_MODEL: &str = "opencode/deepseek-v4.1-flash-free";

/// The OpenCode arguments of one run, after the binary. A turn and a resident task use the
/// same ones: a fresh session, the free model, the `alfred` agent, no approval bypass.
const OPENCODE_RUN_ARGS: &[&str] = &[
    "run",
    "--standalone",
    "--model",
    OPENCODE_ENGINE_MODEL,
    "--agent",
    "alfred",
];

/// The Codex arguments of one run, after the binary: a read-only sandbox, no shell and
/// no approval bypass. A turn and a resident task use the same ones.
const CODEX_RUN_ARGS: &[&str] = &[
    "-p",
    "alfred",
    "exec",
    "--skip-git-repo-check",
    "--color",
    "never",
    "-s",
    "read-only",
    "-a",
    "never",
    "--disable",
    "shell_tool",
];

mod render;
#[cfg(test)]
mod tests;

pub use render::claude_settings;
use render::{claude_mcp, codex_config, test_mcp};
pub(crate) use render::{opencode_config, OpenCodeRun};

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
    /// The hook surface of the host and the token of this run. The hook token
    /// is the only credential that reaches the approval routes.
    pub hook_url: &'a str,
    pub hook_token: &'a str,
    /// The `excubia-uac-hook` binary the CLI runs before each shell command.
    pub hook_command: &'a Path,
    /// The OpenCode plugin directory that gates its shell commands. Only the
    /// OpenCode configuration renders it; the seat stays non-blocking until F-D.
    pub plugin_dir: Option<&'a Path>,
    /// Folders that hold this turn's attachments. Only the restricted Claude CLI
    /// needs them: it reads files only under its working directory.
    pub attachment_dirs: &'a [PathBuf],
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
    /// Where the UAC hook asks, and the hook itself: a project seat edits files
    /// and runs commands, so every tool it uses goes through the approval gate.
    pub hook_url: &'a str,
    pub hook_command: &'a Path,
    /// The OpenCode plugin, only when its canary proved the gate `Safe` for this host.
    /// Without it the OpenCode seat keeps shell denied, as a turn does.
    pub plugin_dir: Option<&'a Path>,
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
            excubia_persist::write_atomic(path, bytes)?;
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
            for dir in input.attachment_dirs {
                args.extend(["--add-dir".to_string(), dir.to_string_lossy().to_string()]);
            }
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
            args.extend(CODEX_RUN_ARGS.iter().map(|arg| arg.to_string()));
        }
        PromptDelivery::AgentConfig => {
            let config_dir = input.run_dir.join("opencode");
            let config = config_dir.join("opencode.json");
            let run = OpenCodeRun::Seat {
                command: &mcp_command,
                tool_url: input.tool_url,
                role: input.role,
            };
            files.push((config, opencode_config(input.plugin_dir, run)?));
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
            // shortcut: a fixed free model, because the account default is paid and can run out of balance, upgrade with a per-device engine model setting.
            args.extend(OPENCODE_RUN_ARGS.iter().map(|arg| arg.to_string()));
            // No `--session`: every turn is a new OpenCode session. A reused session
            // kept failing with `provider.free-limit` on the free tier, while fresh
            // sessions with the same request passed. Alfred's context page carries the chat.
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

/// Build a resident panel plan for a profile that declares resident mode. Claude runs
/// its own `-p`; Codex and OpenCode run through `excubia-mcp resident`, one CLI process
/// per task. The real CLIs' plans are checked by their flags and config, not by a run.
pub fn prepare_resident(input: &ResidentInput<'_>) -> Result<LaunchPlan, LaunchError> {
    if !input.profile.supports_resident() {
        return Err(LaunchError::Unsupported(format!(
            "{} has no resident panel mode",
            input.profile.id.as_str()
        )));
    }
    match input.profile.prompt {
        PromptDelivery::SystemPromptFile => claude_resident(input),
        PromptDelivery::DeveloperInstructions => codex_resident(input),
        PromptDelivery::AgentConfig => opencode_resident(input),
        PromptDelivery::TestStdin => test_resident(input),
    }
}

/// The environment of a resident panel: the seat, its tool surface and its hook. The
/// per-panel token is added by the host, and it reaches the CLI only through this env.
fn resident_env(input: &ResidentInput<'_>) -> Vec<(String, String)> {
    vec![
        ("EXCUBIA_ENV".to_string(), "1".to_string()),
        (
            "EXCUBIA_CHAT_AGENT_ID".to_string(),
            input.agent_id.to_string(),
        ),
        ("EXCUBIA_SEAT_ROLE".to_string(), input.role_id.to_string()),
        ("EXCUBIA_TOOL_URL".to_string(), input.tool_url.to_string()),
        ("EXCUBIA_HOOK_URL".to_string(), input.hook_url.to_string()),
    ]
}

/// `excubia-mcp resident -- <cli> <args…>`: the sidecar that runs the CLI once per task.
fn resident_sidecar(input: &ResidentInput<'_>, cli_args: &[String]) -> (String, Vec<String>) {
    let mut args = vec!["resident".to_string(), "--".to_string()];
    args.push(input.profile.command.clone());
    args.extend(cli_args.iter().cloned());
    (input.mcp_command.to_string_lossy().to_string(), args)
}

/// Codex as a project seat: the same isolated home, read-only sandbox and MCP server as a
/// turn. It receives tasks and closes them through MCP; it cannot run shell, and it
/// cannot write outside the read-only sandbox, which is its normal limit.
fn codex_resident(input: &ResidentInput<'_>) -> Result<LaunchPlan, LaunchError> {
    let mcp_command = input.mcp_command.to_string_lossy().to_string();
    let codex_home = input.seat_dir.join("codex");
    let cli_args: Vec<String> = CODEX_RUN_ARGS.iter().map(|arg| arg.to_string()).collect();
    let (program, args) = resident_sidecar(input, &cli_args);
    let mut env = resident_env(input);
    env.push((
        "CODEX_HOME".to_string(),
        codex_home.to_string_lossy().to_string(),
    ));
    Ok(LaunchPlan {
        program,
        args,
        env,
        cwd: input.seat_dir.to_path_buf(),
        stdin: String::new(),
        files: vec![(
            codex_home.join("config.toml"),
            codex_config(&mcp_command, input.tool_url, input.role)?,
        )],
    })
}

/// OpenCode as a project seat: the same generated configuration as a turn (permissions,
/// `alfred` prompt, MCP server, and the UAC plugin only when its canary proved it). Each
/// task is a fresh OpenCode session, as a turn is.
fn opencode_resident(input: &ResidentInput<'_>) -> Result<LaunchPlan, LaunchError> {
    let mcp_command = input.mcp_command.to_string_lossy().to_string();
    let config_dir = input.seat_dir.join("opencode");
    let run = OpenCodeRun::Seat {
        command: &mcp_command,
        tool_url: input.tool_url,
        role: input.role,
    };
    let cli_args: Vec<String> = OPENCODE_RUN_ARGS.iter().map(|arg| arg.to_string()).collect();
    let (program, args) = resident_sidecar(input, &cli_args);
    let mut env = resident_env(input);
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
    Ok(LaunchPlan {
        program,
        args,
        env,
        cwd: input.seat_dir.to_path_buf(),
        stdin: String::new(),
        files: vec![(
            config_dir.join("opencode.json"),
            opencode_config(input.plugin_dir, run)?,
        )],
    })
}

/// The test adapter's resident panel: it defines the same materialisation, token and MCP
/// transport as the real CLIs, without a model.
fn test_resident(input: &ResidentInput<'_>) -> Result<LaunchPlan, LaunchError> {
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
            ("EXCUBIA_ENV".to_string(), "1".to_string()),
            (
                "EXCUBIA_CHAT_AGENT_ID".to_string(),
                input.agent_id.to_string(),
            ),
            ("EXCUBIA_SEAT_ROLE".to_string(), input.role_id.to_string()),
            ("EXCUBIA_TOOL_URL".to_string(), input.tool_url.to_string()),
        ],
        cwd: input.seat_dir.to_path_buf(),
        stdin: String::new(),
        files: vec![
            (role_file, input.role.as_bytes().to_vec()),
            (mcp, test_mcp(&mcp_command, input.tool_url)?),
        ],
    })
}

/// Claude as a project seat. `excubia-mcp resident` reads each task from the panel
/// and runs one `claude -p` per task in the project folder, with the team tools and
/// the UAC hook; the task's result goes back through `delegation.complete`.
fn claude_resident(input: &ResidentInput<'_>) -> Result<LaunchPlan, LaunchError> {
    if !hook_ready(input.hook_command) {
        return Err(LaunchError::HookMissing(input.hook_command.to_path_buf()));
    }
    let mcp_command = input.mcp_command.to_string_lossy().to_string();
    let role = input.seat_dir.join("role.md");
    let mcp = input.seat_dir.join("claude-mcp.json");
    let settings = input.seat_dir.join("claude-settings.json");
    let mut args = vec!["resident".to_string(), "--".to_string()];
    args.push(input.profile.command.clone());
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
    ]);
    Ok(LaunchPlan {
        program: mcp_command.clone(),
        args,
        env: vec![
            ("EXCUBIA_ENV".to_string(), "1".to_string()),
            (
                "EXCUBIA_CHAT_AGENT_ID".to_string(),
                input.agent_id.to_string(),
            ),
            ("EXCUBIA_SEAT_ROLE".to_string(), input.role_id.to_string()),
            ("EXCUBIA_TOOL_URL".to_string(), input.tool_url.to_string()),
            ("EXCUBIA_HOOK_URL".to_string(), input.hook_url.to_string()),
        ],
        cwd: input.seat_dir.to_path_buf(),
        stdin: String::new(),
        files: vec![
            (role, input.role.as_bytes().to_vec()),
            (mcp, claude_mcp(&mcp_command, input.tool_url)?),
            (
                settings,
                render::claude_settings_ttl(
                    input.hook_command,
                    Some(excubia_chat::MAX_TTL_SECONDS),
                )?,
            ),
        ],
    })
}

fn common_env(input: &LaunchInput<'_>) -> Vec<(String, String)> {
    let mut env = vec![
        ("EXCUBIA_ENV".to_string(), "1".to_string()),
        ("EXCUBIA_CHAT_RUN_ID".to_string(), input.run_id.to_string()),
        // The OpenCode plugin reads `EXCUBIA_RUN_ID`; keep both spellings of the
        // same fact so no adapter has to guess.
        ("EXCUBIA_RUN_ID".to_string(), input.run_id.to_string()),
        (
            "EXCUBIA_CHAT_AGENT_ID".to_string(),
            input.agent_id.to_string(),
        ),
        (
            "EXCUBIA_CHAT_CONVERSATION_ID".to_string(),
            input.conversation_id.to_string(),
        ),
        ("EXCUBIA_TOOL_URL".to_string(), input.tool_url.to_string()),
    ];
    if !input.hook_url.is_empty() {
        env.push(("EXCUBIA_HOOK_URL".to_string(), input.hook_url.to_string()));
        env.push((
            "EXCUBIA_HOOK_TOKEN".to_string(),
            input.hook_token.to_string(),
        ));
    }
    env
}
