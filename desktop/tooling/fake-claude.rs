//! Test-only `claude` adapter: pretends to be a supported Claude Code and
//! translates its MCP config into the shape `fake-cli` reads.
//!
//! It never talks to a provider. Build with:
//!   rustc --edition 2021 -O tooling/fake-claude.rs -o <dir>/claude.exe
//! and put `fake-cli.exe` beside it (or set `HN_FAKE_CLI`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    if let Err(error) = run() {
        eprintln!("fake-claude: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|argument| argument == "--version") {
        println!("claude 2.1.0");
        return Ok(());
    }

    let fake = fake_cli()?;
    let config_index = match args.iter().position(|argument| argument == "--mcp-config") {
        Some(index) => index,
        None => std::process::exit(forward(&fake, &args)?),
    };
    let original = args
        .get(config_index + 1)
        .ok_or("--mcp-config without a path")?;
    let translated = translate(original)?;
    let mut forwarded = args.clone();
    forwarded[config_index + 1] = translated.display().to_string();
    let code = forward(&fake, &forwarded)?;
    let _ = std::fs::remove_file(translated);
    std::process::exit(code);
}

fn forward(fake: &Path, args: &[String]) -> Result<i32, String> {
    let status = Command::new(fake)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| format!("run fake-cli: {error}"))?;
    Ok(status.code().unwrap_or(2))
}

fn fake_cli() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("HN_FAKE_CLI") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let directory = exe.parent().ok_or("no directory")?;
    let sibling = directory.join(if cfg!(windows) {
        "fake-cli.exe"
    } else {
        "fake-cli"
    });
    if sibling.is_file() {
        Ok(sibling)
    } else {
        Err(format!("fake-cli not found at {}", sibling.display()))
    }
}

/// Find the `"hn-core"` server object in `mcpServers` and wrap it as `mcp`.
/// The generated config is small and its strings carry no braces.
fn translate(path: &str) -> Result<PathBuf, String> {
    let raw = std::fs::read_to_string(path).map_err(|error| format!("read MCP config: {error}"))?;
    let servers = raw.find("\"mcpServers\"").ok_or("MCP config has no mcpServers")?;
    let key = raw[servers..]
        .find("\"hn-core\"")
        .map(|offset| servers + offset)
        .ok_or("MCP config has no hn-core server")?;
    let open = raw[key..]
        .find('{')
        .map(|offset| key + offset)
        .ok_or("MCP config server is not an object")?;
    let close = matching_brace(&raw, open).ok_or("unterminated MCP server object")?;
    let translated = format!("{{\"mcp\":{}}}", &raw[open..=close]);
    let target = std::env::temp_dir().join(format!(
        "hn-fake-claude-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default()
    ));
    std::fs::write(&target, translated).map_err(|error| format!("write MCP config: {error}"))?;
    Ok(target)
}

fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for (offset, character) in text[open..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '{' if !quoted => depth += 1,
            '}' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}
