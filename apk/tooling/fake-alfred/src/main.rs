//! `claude.exe` shim for APK5 acceptance: the shipped host only selects its
//! real CLI profiles, so this wrapper answers Claude's `--version`, translates
//! the generated `claude-mcp.json` into the shape `fake-cli.exe` expects and
//! delegates the turn. No model quota is spent.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fake-alfred: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|argument| argument == "--version") {
        println!("2.1.305 (Claude Code)");
        return Ok(());
    }
    let config = flag(&args, "--mcp-config").ok_or("missing --mcp-config")?;
    let raw = std::fs::read_to_string(&config).map_err(|error| error.to_string())?;
    let parsed: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| format!("claude-mcp.json: {error}"))?;
    let server = parsed
        .get("mcpServers")
        .and_then(|servers| servers.get("hn-core"))
        .ok_or("claude-mcp.json has no mcpServers.hn-core")?;
    let translated = serde_json::json!({ "mcp": server });
    let sibling = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .parent()
        .ok_or("the shim has no directory")?
        .to_path_buf();
    let temp = temp_config(&sibling, &translated)?;
    let fake = fake_cli(&sibling)?;
    let mut child = Command::new(fake)
        .arg("--mcp-config")
        .arg(&temp)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("run fake-cli: {error}"))?;
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_string(&mut stderr)
            .map_err(|error| format!("read fake-cli stderr: {error}"))?;
    }
    let status = child
        .wait()
        .map_err(|error| format!("wait fake-cli: {error}"))?;
    log_shim(
        &sibling,
        &format!("args={args:?} status={status} stderr={stderr:?}"),
    );
    eprint!("{stderr}");
    let _ = std::fs::remove_file(&temp);
    if status.success() {
        Ok(())
    } else {
        Err(format!("fake-cli exited with {status}"))
    }
}

fn log_shim(directory: &Path, line: &str) {
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("fake-alfred.log"))
    {
        let _ = writeln!(file, "{line}");
    }
}

fn fake_cli(directory: &Path) -> Result<PathBuf, String> {
    let name = if cfg!(windows) {
        "fake-cli.exe"
    } else {
        "fake-cli"
    };
    let path = directory.join(name);
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!("{} is missing", path.display()))
    }
}

fn temp_config(directory: &Path, value: &serde_json::Value) -> Result<PathBuf, String> {
    let path = directory.join(format!("fake-alfred-{}.json", std::process::id()));
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(path)
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}
