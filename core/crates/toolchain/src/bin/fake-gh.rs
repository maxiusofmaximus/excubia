//! A fixture that answers like `gh` (or `git`, by its file name) for the link and
//! install tests. It is a test tool, never shipped: behaviour comes from env vars.
//!
//! - `FAKE_GH_LOGIN`: `ok` (default), `fragmented` (the code arrives in pieces),
//!   `fail` (code, then exit 1), `hang` (code, then waits until killed).
//! - `FAKE_GH_STATUS`: `ok` (default) or `none` (exit 1).
//! - `FAKE_GH_API`: `ok` (default) or `fail`.
//! - `FAKE_GH_SETUP`: `ok` (default) or `fail`.

use std::fs::OpenOptions;
use std::io::Write;
use std::thread;
use std::time::Duration;

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn stderr_piece(text: &str) {
    let mut err = std::io::stderr();
    let _ = err.write_all(text.as_bytes());
    let _ = err.flush();
    thread::sleep(Duration::from_millis(30));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let stem = std::path::Path::new(&args[0])
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let rest: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();

    if stem.starts_with("git") {
        git(&rest);
        return;
    }
    gh(&rest);
}

fn git(args: &[&str]) {
    match args {
        ["--version"] => println!("git version 2.56.1.windows.1"),
        // Must come first: `--get <key>` has the shape of a `<key> <value>` write.
        // Reads a key the way `git config --file <path> --get <key>` does for this fixture's file.
        ["config", "--file", path, "--get", key] => {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let prefix = format!("{key} = ");
            match text.lines().find_map(|line| line.strip_prefix(&prefix)) {
                Some(value) => println!("{value}"),
                None => std::process::exit(1),
            }
        }
        ["config", "--file", path, key, value] => {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .expect("open config");
            writeln!(file, "{key} = {value}").expect("write config");
        }
        _ => std::process::exit(2),
    }
}

fn gh(args: &[&str]) {
    match args {
        ["--version"] => println!("gh version 2.105.0 (fixture)"),
        ["auth", "login", ..] => login(),
        ["auth", "status", ..] => {
            if env("FAKE_GH_STATUS", "ok") == "ok" {
                println!("ok");
            } else {
                std::process::exit(1);
            }
        }
        ["api", ..] => {
            if env("FAKE_GH_API", "ok") == "fail" {
                std::process::exit(1);
            }
            // The email is here on purpose: the app must never read or store it.
            println!(
                "{{\"id\":4242,\"login\":\"octo\",\"name\":\"Octo Cat\",\"email\":\"private@example.com\"}}"
            );
        }
        ["auth", "setup-git", ..] => {
            if env("FAKE_GH_SETUP", "ok") == "fail" {
                std::process::exit(1);
            }
            // Like gh: the credential helper goes to the profile's Git file, nowhere else.
            let path = env("GIT_CONFIG_GLOBAL", "");
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .expect("open profile git config");
            writeln!(file, "credential.https://github.com.helper = !gh auth git-credential")
                .expect("write helper");
        }
        _ => std::process::exit(2),
    }
}

fn login() {
    match env("FAKE_GH_LOGIN", "ok").as_str() {
        "fragmented" => {
            stderr_piece("! First copy your ");
            stderr_piece("one-time code: AB");
            stderr_piece("CD-12");
            stderr_piece("34\n");
            stderr_piece("Open https://github.com/login/device\n");
        }
        "hang" => {
            stderr_piece("First copy your one-time code: WXYZ-9876\n");
            thread::sleep(Duration::from_secs(600));
        }
        "fail" => {
            stderr_piece("First copy your one-time code: QWER-1234\n");
            std::process::exit(1);
        }
        "garbage" => {
            stderr_piece("unexpected output without a code\n");
            std::process::exit(1);
        }
        _ => stderr_piece("First copy your one-time code: ABCD-1234\n"),
    }
    if env("FAKE_GH_LOGIN", "ok") != "hang" && env("FAKE_GH_LOGIN", "ok") != "fail" {
        std::process::exit(0);
    }
    std::process::exit(1);
}
