//! Launch plans: what each CLI is given, and the files it reads.

use super::render::CLAUDE_HOOK_TIMEOUT_SECONDS;
use super::*;
use crate::hook_action::CLAUDE_HOOK_MATCHER;
use crate::profile::{test_profile, McpDeclaration};
use hn_core_test_dir::TestDir;
use std::path::PathBuf;

fn input<'a>(
    profile: &'a CliProfile,
    root: &'a Path,
    role: &'a str,
    prompt: &'a str,
) -> LaunchInput<'a> {
    LaunchInput {
        profile,
        run_id: "run_1",
        agent_id: "ag_alfred",
        conversation_id: "cnv_1",
        run_dir: root,
        role,
        prompt,
        tool_url: "http://127.0.0.1:9/internal/chat/tools",
        mcp_command: Path::new("C:\\bin\\hn-mcp.exe"),
        session_id: "alfred",
        hook_url: "http://127.0.0.1:9",
        hook_token: "test-hook-token",
        hook_command: Path::new("C:\\bin\\hn-uac-hook.exe"),
    }
}

fn file_text(plan: &LaunchPlan, name: &str) -> String {
    let (_, bytes) = plan
        .files
        .iter()
        .find(|(path, _)| path.ends_with(name))
        .expect("file");
    String::from_utf8(bytes.clone()).expect("utf8")
}

#[test]
fn test_plan_has_config_and_no_token() {
    let profile = test_profile("fake-cli.exe");
    let root = PathBuf::from("run-dir");
    let plan = prepare(&input(&profile, &root, "role", "prompt")).expect("plan");
    assert_eq!(plan.program, "fake-cli.exe");
    assert!(plan.args.iter().any(|arg| arg == "--mcp-config"));
    assert_eq!(plan.stdin, "prompt");
    let config = file_text(&plan, "mcp.json");
    assert!(config.contains("hn-mcp.exe"));
    assert!(!config.contains("token"));
    assert!(!plan
        .files
        .iter()
        .any(|(_, bytes)| String::from_utf8_lossy(bytes).contains("HN_TOOL_TOKEN")));
}

/// A stand-in hook binary: the Claude launch refuses a hook it cannot find.
fn hook_in(dir: &Path) -> PathBuf {
    let path = dir.join("hn-uac-hook.exe");
    std::fs::write(&path, b"").expect("hook");
    path
}

#[test]
fn claude_plan_isolates_settings_and_mcp() {
    let profile = crate::profile::shipped().remove(0);
    let scratch = TestDir::new("seat-claude");
    let hook = hook_in(&scratch);
    let root = PathBuf::from("run-dir");
    let mut launch = input(&profile, &root, "role", "prompt");
    launch.hook_command = &hook;
    let plan = prepare(&launch).expect("plan");
    for flag in [
        "-p",
        "--strict-mcp-config",
        "--setting-sources",
        "--restricted",
    ] {
        assert!(plan.args.iter().any(|arg| arg == flag), "missing {flag}");
    }
    let mcp: serde_json::Value =
        serde_json::from_str(&file_text(&plan, "claude-mcp.json")).expect("json");
    assert_eq!(
        mcp["mcpServers"]["hn-core"]["command"],
        "C:\\bin\\hn-mcp.exe"
    );
}

#[test]
fn claude_settings_gate_every_edit_tool_with_the_blocking_hook() {
    let profile = crate::profile::shipped().remove(0);
    let scratch = TestDir::new("seat-claude-gate");
    let hook_bin = hook_in(&scratch);
    let root = PathBuf::from("run-dir");
    let mut launch = input(&profile, &root, "role", "prompt");
    launch.hook_command = &hook_bin;
    let plan = prepare(&launch).expect("plan");
    let settings: serde_json::Value =
        serde_json::from_str(&file_text(&plan, "claude-settings.json")).expect("json");
    let allowed = settings["permissions"]["allow"].to_string();
    for tool in ["Bash", "Write", "Edit", "WebFetch"] {
        assert!(
            allowed.contains(tool),
            "{tool} must be allowed: the hook decides"
        );
    }
    let hook = &settings["hooks"]["PreToolUse"][0];
    assert_eq!(hook["matcher"], CLAUDE_HOOK_MATCHER);
    assert_eq!(hook["hooks"][0]["type"], "command");
    assert_eq!(
        hook["hooks"][0]["command"],
        format!("\"{}\"", hook_bin.to_string_lossy())
    );
    assert_eq!(hook["hooks"][0]["timeout"], CLAUDE_HOOK_TIMEOUT_SECONDS);
    assert!(!file_text(&plan, "claude-settings.json").contains("test-hook-token"));
}

#[test]
fn a_claude_launch_without_its_hook_is_refused() {
    let profile = crate::profile::shipped().remove(0);
    let scratch = TestDir::new("seat-claude-nohook");
    let root = PathBuf::from("run-dir");
    let absent = scratch.join("hn-uac-hook.exe");
    let mut launch = input(&profile, &root, "role", "prompt");
    launch.hook_command = &absent;
    let error = prepare(&launch).expect_err("no hook, no launch");
    assert!(matches!(error, LaunchError::HookMissing(_)));
}

#[test]
fn a_directory_is_not_a_runnable_hook() {
    let scratch = TestDir::new("seat-hook-dir");
    assert!(!hook_ready(&scratch));
    assert!(!hook_ready(&scratch.join("absent.exe")));
    assert!(hook_ready(&hook_in(&scratch)));
}

#[test]
fn codex_plan_uses_an_isolated_home() {
    let profile = crate::profile::shipped().remove(1);
    let root = PathBuf::from("run-dir");
    let plan = prepare(&input(&profile, &root, "role", "prompt")).expect("plan");
    assert!(plan.env.iter().any(|(key, _)| key == "CODEX_HOME"));
    let config: toml::Value = toml::from_str(&file_text(&plan, "config.toml")).expect("toml");
    assert_eq!(
        config["mcp_servers"]["hn-core"]["command"].as_str(),
        Some("C:\\bin\\hn-mcp.exe")
    );
    assert_eq!(
        config["profiles"]["alfred"]["developer_instructions"].as_str(),
        Some("role")
    );
}

#[test]
fn opencode_plan_sets_agent_prompt_and_session() {
    let profile = crate::profile::shipped().remove(2);
    let root = PathBuf::from("run-dir");
    let plan = prepare(&input(&profile, &root, "role", "prompt")).expect("plan");
    assert!(plan.args.iter().any(|arg| arg == "--session"));
    let config: serde_json::Value =
        serde_json::from_str(&file_text(&plan, "opencode.json")).expect("json");
    assert_eq!(config["agent"]["alfred"]["prompt"], "role");
    assert_eq!(
        config["mcp"]["servers"]["hn-core"]["environment"]["HN_TOOL_URL"],
        "http://127.0.0.1:9/internal/chat/tools"
    );
}

#[test]
fn materialise_writes_the_files_atomically() {
    let dir = TestDir::new("seat-launch");
    let profile = test_profile("fake-cli.exe");
    let plan = prepare(&input(&profile, &dir, "role", "prompt")).expect("plan");
    plan.materialise().expect("materialise");
    assert!(dir.join("mcp.json").is_file());
}

#[test]
fn an_inconsistent_profile_is_rejected_before_writing_files() {
    let mut profile = test_profile("fake-cli.exe");
    profile.mcp = McpDeclaration::ClaudeJson;
    let scratch = TestDir::new("seat-bad");
    let dir = scratch.join("absent");
    let error = prepare(&input(&profile, &dir, "role", "prompt")).expect_err("rejected");
    assert!(matches!(error, LaunchError::Unsupported(_)));
    assert!(!dir.exists());
}
