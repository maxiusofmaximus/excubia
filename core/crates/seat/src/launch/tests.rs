//! Launch plans: what each CLI is given, and the files it reads.

use super::render::CLAUDE_HOOK_TIMEOUT_SECONDS;
use super::*;
use crate::hook_action::CLAUDE_HOOK_MATCHER;
use crate::profile::{test_profile, McpDeclaration};
use excubia_test_dir::TestDir;
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
        mcp_command: Path::new("C:\\bin\\excubia-mcp.exe"),
        hook_url: "http://127.0.0.1:9",
        hook_token: "test-hook-token",
        hook_command: Path::new("C:\\bin\\excubia-uac-hook.exe"),
        plugin_dir: None,
        attachment_dirs: &[],
        engine_model: None,
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
    assert!(config.contains("excubia-mcp.exe"));
    assert!(!config.contains("token"));
    assert!(!plan
        .files
        .iter()
        .any(|(_, bytes)| String::from_utf8_lossy(bytes).contains("EXCUBIA_TOOL_TOKEN")));
}

/// A stand-in hook binary: the Claude launch refuses a hook it cannot find.
fn hook_in(dir: &Path) -> PathBuf {
    let path = dir.join("excubia-uac-hook.exe");
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
        mcp["mcpServers"]["excubia-core"]["command"],
        "C:\\bin\\excubia-mcp.exe"
    );
}

#[test]
fn claude_plan_grants_the_attachment_folder_only_when_there_are_attachments() {
    let profile = crate::profile::shipped().remove(0);
    let scratch = TestDir::new("seat-claude-attachments");
    let hook = hook_in(&scratch);
    let root = PathBuf::from("run-dir");
    let folder = PathBuf::from("data/chat/attachments/cnv_1");
    let dirs = [folder.clone()];
    let mut launch = input(&profile, &root, "role", "prompt");
    launch.hook_command = &hook;
    launch.attachment_dirs = &dirs;
    let plan = prepare(&launch).expect("plan");
    let flag = plan
        .args
        .iter()
        .position(|arg| arg == "--add-dir")
        .expect("--add-dir");
    assert_eq!(plan.args[flag + 1], folder.to_string_lossy());

    launch.attachment_dirs = &[];
    let plan = prepare(&launch).expect("plan");
    assert!(!plan.args.iter().any(|arg| arg == "--add-dir"));
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
    for tool in ["Bash", "Write", "Edit", "WebFetch", "WebSearch"] {
        assert!(
            allowed.contains(tool),
            "{tool} must be allowed: the hook decides"
        );
    }
    // A web search is a card the operator answers, never a standing denial.
    assert!(settings["permissions"].get("deny").is_none());
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
fn claude_allows_every_tool_the_hook_gates() {
    // Claude runs the hook first; a gated tool missing from `allow` is refused even after the operator approves it.
    let profile = crate::profile::shipped().remove(0);
    let scratch = TestDir::new("seat-claude-allow-matcher");
    let hook_bin = hook_in(&scratch);
    let root = PathBuf::from("run-dir");
    let mut launch = input(&profile, &root, "role", "prompt");
    launch.hook_command = &hook_bin;
    let plan = prepare(&launch).expect("plan");
    let settings: serde_json::Value =
        serde_json::from_str(&file_text(&plan, "claude-settings.json")).expect("json");
    let allowed: Vec<&str> = settings["permissions"]["allow"]
        .as_array()
        .expect("allow list")
        .iter()
        .filter_map(|rule| rule.as_str())
        .collect();
    for tool in CLAUDE_HOOK_MATCHER.split('|') {
        assert!(allowed.contains(&tool), "{tool} is gated by the hook but not allowed");
    }
}

#[test]
fn claude_allows_each_tool_the_seat_serves() {
    let profile = crate::profile::shipped().remove(0);
    let scratch = TestDir::new("seat-claude-chat-tools");
    let hook_bin = hook_in(&scratch);
    let root = PathBuf::from("run-dir");
    let mut launch = input(&profile, &root, "role", "prompt");
    launch.hook_command = &hook_bin;
    let plan = prepare(&launch).expect("plan");
    let settings: serde_json::Value =
        serde_json::from_str(&file_text(&plan, "claude-settings.json")).expect("json");
    let allowed = settings["permissions"]["allow"].to_string();
    let served = crate::tools::definitions();
    for tool in served["tools"].as_array().expect("tools") {
        let name = tool["name"].as_str().expect("name");
        // Without this rule `claude -p` refuses the call and the turn ends silently.
        let rule = format!("mcp__excubia-core__{}", name.replace('.', "_"));
        assert!(allowed.contains(&rule), "{rule} must be allowed");
    }
}

#[test]
fn a_claude_launch_without_its_hook_is_refused() {
    let profile = crate::profile::shipped().remove(0);
    let scratch = TestDir::new("seat-claude-nohook");
    let root = PathBuf::from("run-dir");
    let absent = scratch.join("excubia-uac-hook.exe");
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
        config["mcp_servers"]["excubia-core"]["command"].as_str(),
        Some("C:\\bin\\excubia-mcp.exe")
    );
    assert_eq!(
        config["profiles"]["alfred"]["developer_instructions"].as_str(),
        Some("role")
    );
}

#[test]
fn opencode_plan_sets_agent_prompt_without_a_session() {
    let profile = crate::profile::shipped().remove(2);
    let root = PathBuf::from("run-dir");
    let plan = prepare(&input(&profile, &root, "role", "prompt")).expect("plan");
    // Every turn is a fresh OpenCode session: no `--session` argument.
    assert!(
        !plan.args.iter().any(|arg| arg == "--session"),
        "{:?}",
        plan.args
    );
    let config: serde_json::Value =
        serde_json::from_str(&file_text(&plan, "opencode.json")).expect("json");
    assert_eq!(config["agent"]["alfred"]["prompt"], "role");
    assert_eq!(config["model"], "opencode/crouton-alpha-free");
    assert_eq!(
        config["mcp"]["excubia-core"]["environment"]["EXCUBIA_TOOL_URL"],
        "http://127.0.0.1:9/internal/chat/tools"
    );
    // Native shell and edit are refused. No plugin directory means no plugin key.
    assert_eq!(config["permission"]["bash"], "deny");
    assert_eq!(config["permission"]["edit"], "deny");
    assert!(config.get("plugin").is_none());
}

#[test]
fn an_opencode_turn_runs_on_the_free_model_it_is_given() {
    let profile = crate::profile::shipped().remove(2);
    let root = PathBuf::from("run-dir");
    let mut turn = input(&profile, &root, "role", "prompt");
    turn.engine_model = Some(FREE_MODELS[1]);
    let plan = prepare(&turn).expect("plan");
    let at = plan.args.iter().position(|arg| arg == "--model").expect("--model");
    assert_eq!(plan.args[at + 1], FREE_MODELS[1]);
    assert!(!plan.args.iter().any(|arg| arg == FREE_MODELS[0]), "{:?}", plan.args);
}

#[test]
fn opencode_turns_run_in_their_own_folder() {
    // No session is kept, so nothing binds a turn to a folder that outlives it.
    let profile = crate::profile::shipped().remove(2);
    let root = PathBuf::from("seat").join("runs").join("turn-1");
    let plan = prepare(&input(&profile, &root, "role", "prompt")).expect("plan");
    assert_eq!(plan.cwd, root);
}

#[test]
fn opencode_plan_registers_the_plugin_by_absolute_path() {
    let profile = crate::profile::shipped().remove(2);
    let root = PathBuf::from("run-dir");
    let plugin = PathBuf::from("tooling").join("opencode-uac-plugin");
    let mut launch = input(&profile, &root, "role", "prompt");
    launch.plugin_dir = Some(&plugin);
    let plan = prepare(&launch).expect("plan");
    let config: serde_json::Value =
        serde_json::from_str(&file_text(&plan, "opencode.json")).expect("json");
    let registered = config["plugin"][0].as_str().expect("plugin path");
    assert!(Path::new(registered).is_absolute(), "{registered}");
    assert!(registered.ends_with("opencode-uac-plugin"), "{registered}");
    // With the plugin, shell runs and the plugin decides; edit and fetch stay denied.
    assert_eq!(config["permission"]["bash"], "allow");
    assert_eq!(config["permission"]["edit"], "deny");
}

#[test]
fn opencode_without_a_plugin_keeps_shell_denied() {
    let profile = crate::profile::shipped().remove(2);
    let root = PathBuf::from("run-dir");
    let plan = prepare(&input(&profile, &root, "role", "prompt")).expect("plan");
    let config: serde_json::Value =
        serde_json::from_str(&file_text(&plan, "opencode.json")).expect("json");
    assert_eq!(config["permission"]["bash"], "deny");
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

fn resident<'a>(
    profile: &'a CliProfile,
    seat_dir: &'a Path,
    role: &'a str,
    plugin: Option<&'a Path>,
) -> ResidentInput<'a> {
    ResidentInput {
        profile,
        agent_id: "ag_lead",
        role_id: "lead",
        seat_dir,
        role,
        tool_url: "http://127.0.0.1:9/internal/chat/tools",
        mcp_command: Path::new("C:\\bin\\excubia-mcp.exe"),
        hook_url: "http://127.0.0.1:9",
        hook_command: existing_hook(),
        plugin_dir: plugin,
    }
}

/// The resident plan checks that the hook file exists and is runnable; the running test
/// binary always is, so it stands in for the hook.
fn existing_hook() -> &'static Path {
    static HOOK: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    HOOK.get_or_init(|| std::env::current_exe().expect("test binary"))
        .as_path()
}

/// The turn plan of the same profile: a resident task must run the CLI with the same flags.
fn turn_args(profile: &CliProfile) -> Vec<String> {
    let root = PathBuf::from("run-dir");
    prepare(&input(profile, &root, "role", "prompt"))
        .expect("turn plan")
        .args
}

#[test]
fn every_shipped_cli_declares_resident_mode_and_the_plan_follows_the_predicate() {
    let mcp = Path::new("C:\\bin\\excubia-mcp.exe");
    for profile in crate::profile::shipped() {
        assert!(profile.supports_resident(), "{}", profile.id.as_str());
        let seat = PathBuf::from("seat").join("resident");
        let plan = prepare_resident(&resident(&profile, &seat, "role", None))
            .unwrap_or_else(|error| panic!("{}: {error}", profile.id.as_str()));
        assert_eq!(plan.cwd, seat);
        if profile.prompt == PromptDelivery::SystemPromptFile {
            // Claude keeps its own resident command: `excubia-mcp resident -- claude -p …`.
            assert_eq!(plan.program, mcp.to_string_lossy());
            assert_eq!(&plan.args[..3], ["resident", "--", "claude"]);
        } else {
            assert_eq!(plan.program, mcp.to_string_lossy(), "{}", profile.id.as_str());
            assert_eq!(&plan.args[..2], ["resident", "--"], "{}", profile.id.as_str());
            assert_eq!(plan.args[2], profile.command, "{}", profile.id.as_str());
        }
    }
}

#[test]
fn a_codex_resident_task_runs_the_turn_flags_in_a_read_only_sandbox() {
    let profile = crate::profile::shipped().remove(1);
    let seat = PathBuf::from("seat").join("resident");
    let plan = prepare_resident(&resident(&profile, &seat, "rol líder", None)).expect("plan");
    let turn = turn_args(&profile);
    // The CLI's own arguments are the turn's, byte for byte.
    assert_eq!(&plan.args[3..], turn.as_slice());
    let joined = plan.args.join(" ");
    assert!(joined.contains("-s read-only"), "{joined}");
    assert!(joined.contains("--disable shell_tool"), "{joined}");
    // No approval bypass or profile of the host's own.
    for banned in ["bypass", "full-auto", "yolo", "dangerously", "--profile "] {
        assert!(!joined.contains(banned), "{joined}");
    }
    let home = plan
        .env
        .iter()
        .find(|(key, _)| key == "CODEX_HOME")
        .expect("isolated home");
    assert_eq!(home.1, seat.join("codex").to_string_lossy());
    let config: toml::Value = toml::from_str(&file_text(&plan, "config.toml")).expect("toml");
    assert_eq!(
        config["profiles"]["alfred"]["developer_instructions"].as_str(),
        Some("rol líder")
    );
    assert_eq!(
        config["mcp_servers"]["excubia-core"]["command"].as_str(),
        Some("C:\\bin\\excubia-mcp.exe")
    );
}

#[test]
fn an_opencode_resident_task_runs_the_turn_flags_and_keeps_shell_denied_without_the_plugin() {
    let profile = crate::profile::shipped().remove(2);
    let seat = PathBuf::from("seat").join("resident");
    let plan = prepare_resident(&resident(&profile, &seat, "role", None)).expect("plan");
    assert_eq!(&plan.args[3..], turn_args(&profile).as_slice());
    assert!(!plan.args.iter().any(|arg| arg == "--session"), "{:?}", plan.args);
    assert!(plan.args.iter().any(|arg| arg == "opencode/deepseek-v4.1-flash-free"));
    let config: serde_json::Value =
        serde_json::from_str(&file_text(&plan, "opencode.json")).expect("json");
    assert_eq!(config["agent"]["alfred"]["prompt"], "role");
    assert_eq!(config["permission"]["bash"], "deny");
    assert_eq!(config["permission"]["edit"], "deny");
    assert!(config.get("plugin").is_none());
    assert!(plan
        .env
        .iter()
        .any(|(key, value)| key == "OPENCODE_CONFIG_DIR" && value.ends_with("opencode")));
}

#[test]
fn an_opencode_resident_panel_gets_the_plugin_only_when_it_is_given_one() {
    let profile = crate::profile::shipped().remove(2);
    let seat = PathBuf::from("seat").join("resident");
    let plugin = PathBuf::from("tooling").join("opencode-uac-plugin");
    let plan = prepare_resident(&resident(&profile, &seat, "role", Some(&plugin))).expect("plan");
    let config: serde_json::Value =
        serde_json::from_str(&file_text(&plan, "opencode.json")).expect("json");
    let registered = config["plugin"][0].as_str().expect("plugin path");
    assert!(Path::new(registered).is_absolute(), "{registered}");
    assert_eq!(config["permission"]["bash"], "allow");
    assert_eq!(config["permission"]["edit"], "deny");
}

#[test]
fn no_resident_plan_writes_the_token_or_the_hook_token_into_a_file() {
    for profile in crate::profile::shipped() {
        let seat = PathBuf::from("seat").join("resident");
        let plan = prepare_resident(&resident(&profile, &seat, "role", None)).expect("plan");
        assert!(!plan.env.iter().any(|(key, _)| key == "EXCUBIA_TOOL_TOKEN"));
        assert!(!plan.env.iter().any(|(key, _)| key == "EXCUBIA_HOOK_TOKEN"));
        for (_, bytes) in &plan.files {
            let text = String::from_utf8_lossy(bytes);
            assert!(!text.contains("EXCUBIA_TOOL_TOKEN"), "{}", profile.id.as_str());
            assert!(!text.contains("EXCUBIA_HOOK_TOKEN"), "{}", profile.id.as_str());
        }
    }
}

#[test]
fn a_profile_without_resident_mode_is_refused_before_any_file() {
    let mut profile = crate::profile::shipped().remove(1);
    profile.resident = false;
    assert!(!profile.supports_resident());
    let scratch = TestDir::new("seat-no-resident");
    let seat = scratch.join("absent");
    let error = prepare_resident(&resident(&profile, &seat, "role", None)).expect_err("refused");
    assert!(
        matches!(&error, LaunchError::Unsupported(text) if text.contains("no resident panel mode")),
        "{error}"
    );
    assert!(!seat.exists());
}

#[test]
fn the_resident_predicate_is_false_for_an_inconsistent_profile() {
    let mut profile = crate::profile::shipped().remove(0);
    profile.mcp = McpDeclaration::CodexToml;
    assert!(!profile.supports_resident());
    let seat = PathBuf::from("seat").join("resident");
    assert!(prepare_resident(&resident(&profile, &seat, "role", None)).is_err());
}
