use std::path::PathBuf;
use std::process::Command;

/// The UAC canary: the simulated loader of OpenCode. The plugin must exist
/// and must declare a hook; when it does, the plugin blocks the test command
/// and the canary says so. When it does not load, the command runs and the
/// canary fails: that is the case the preflight must catch.
pub(super) fn uac_canary(plugin_dir: &str) -> Result<(), String> {
    let index = std::path::Path::new(plugin_dir).join("index.js");
    let source = match std::fs::read_to_string(&index) {
        Ok(source) => source,
        Err(_) => {
            eprintln!("failed to load plugin: {}", index.display());
            println!("excubia-uac-canary: the command ran");
            return Ok(());
        }
    };
    if !source.contains("tool.hook") && !source.contains("server(") {
        eprintln!("failed to load plugin: {} has no hook", index.display());
        println!("excubia-uac-canary: the command ran");
        return Ok(());
    }
    eprintln!("[UAC] aprobación denegada por el operador. Acción denegada (fail closed).");
    std::process::exit(2);
}

/// The blocking hook of the agent: the same binary a real CLI would run
/// before a risky action. Its exit code is the answer.
pub(super) fn run_uac_hook(task: &str, ttl: &str) -> Result<String, String> {
    let hook = hook_path()?;
    let cwd = std::env::current_dir()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    let output = Command::new(&hook)
        .args([
            "--kind",
            "command",
            "--tool",
            "bash",
            "--summary",
            task,
            "--args",
            "{\"command\":\"cargo run\"}",
            "--cwd",
            &cwd,
            "--ttl",
            ttl,
        ])
        .output()
        .map_err(|error| format!("the UAC hook could not run: {error}"))?;
    Ok(match output.status.code() {
        Some(0) => "allowed".to_string(),
        Some(2) => "denied".to_string(),
        _ => "no decision".to_string(),
    })
}

/// `[[fake:uac-ttl=1]]` asks for a short approval; the default is 30 seconds.
pub(super) fn uac_ttl(text: &str) -> &str {
    text.split("[[fake:uac-ttl=")
        .nth(1)
        .and_then(|rest| rest.split(']').next())
        .unwrap_or("30")
}

/// The hook binary: the environment names it, or it sits beside this one.
pub(super) fn hook_path() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("EXCUBIA_UAC_HOOK") {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let directory = executable
        .parent()
        .ok_or_else(|| "the fake CLI has no directory".to_string())?;
    let name = if cfg!(windows) {
        "excubia-uac-hook.exe"
    } else {
        "excubia-uac-hook"
    };
    Ok(directory.join(name))
}
