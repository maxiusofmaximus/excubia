//! Parser tests.

use super::*;
use std::path::PathBuf;
use std::time::Duration;

fn args(items: &[&str]) -> impl Iterator<Item = String> {
    items
        .iter()
        .map(|item| item.to_string())
        .collect::<Vec<_>>()
        .into_iter()
}

#[test]
fn the_daemon_defaults_are_loopback_and_a_single_label() {
    let Command::Run(options) = parse(args(&[])).expect("parse") else {
        panic!("expected run");
    };
    assert_eq!(options.bind.to_string(), "127.0.0.1");
    assert_eq!(options.port, 8791);
    assert!(options.spawns.is_empty());
    assert_eq!(options.hook_ttl, crate::DEFAULT_HOOK_TTL);
    assert_eq!(options.snapshot_lines, crate::DEFAULT_SNAPSHOT_LINES);
}

#[test]
fn the_daemon_accepts_the_hook_and_snapshot_knobs() {
    let Command::Run(options) = parse(args(&[
        "--hook-ttl",
        "10",
        "--snapshot-lines",
        "120",
        "--data-dir",
        "C:/tmp/hn",
    ]))
    .expect("parse") else {
        panic!("expected run");
    };
    assert_eq!(options.hook_ttl, Duration::from_secs(10));
    assert_eq!(options.snapshot_lines, 120);
    assert!(parse(args(&["--snapshot-lines", "500"])).is_err());
}

#[test]
fn hook_token_is_a_subcommand() {
    let Command::HookToken { seat, data_dir } =
        parse(args(&["hook-token", "--data-dir", "C:/tmp/hn"])).expect("parse")
    else {
        panic!("expected hook-token");
    };
    assert!(seat.is_none(), "the host token is the default");
    assert!(data_dir.ends_with("hn"));
}

#[test]
fn hook_token_can_name_a_seat() {
    let Command::HookToken { seat, .. } = parse(args(&[
        "hook-token",
        "--seat",
        "ag_1",
        "--data-dir",
        "C:/tmp/hn",
    ]))
    .expect("parse") else {
        panic!("expected hook-token");
    };
    assert_eq!(seat.as_deref(), Some("ag_1"));
}

#[test]
fn claude_settings_needs_the_hook_and_takes_an_optional_out() {
    let Command::ClaudeSettings { hook, out } = parse(args(&[
        "claude-settings",
        "--hook",
        "C:/bin/excubia-uac-hook.exe",
        "--out",
        "C:/tmp/settings.json",
    ]))
    .expect("parse") else {
        panic!("expected claude-settings");
    };
    assert!(hook.ends_with("excubia-uac-hook.exe"));
    assert!(out.is_some_and(|path| path.ends_with("settings.json")));
    assert!(
        parse(args(&["claude-settings"])).is_err(),
        "--hook is required"
    );
}

/// The canonical hint the contracts and the clients quote must be exactly
/// the usage line the CLI prints, must parse with the real CLI, and must
/// list exactly the permissions the core knows.
#[test]
fn the_grant_hint_matches_the_cli_and_the_permission_labels() {
    assert!(
        USAGE.lines().any(|line| line.trim() == GRANT_HINT),
        "USAGE must contain the canonical grant hint verbatim: {GRANT_HINT}"
    );

    let Command::Devices { command, .. } = parse(args(&[
        "devices",
        "grant",
        "dev-abc",
        "--permission",
        "respond",
        "--data-dir",
        "C:/tmp/hn",
    ]))
    .expect("parse") else {
        panic!("expected devices");
    };
    let DevicesCommand::Grant {
        device_id,
        permission,
    } = command
    else {
        panic!("expected grant");
    };
    assert_eq!(device_id, "dev-abc");
    assert_eq!(permission, "respond");

    let placeholder = GRANT_HINT
        .split("--permission <")
        .nth(1)
        .and_then(|rest| rest.split('>').next())
        .expect("permission placeholder");
    let mut hinted: Vec<&str> = placeholder.split('|').collect();
    hinted.sort_unstable();
    let mut labels: Vec<&str> = hn_core_auth::Permission::ALL
        .iter()
        .map(|permission| permission.label())
        .collect();
    labels.sort_unstable();
    assert_eq!(hinted, labels, "the hint must list every permission id");
}

#[test]
fn the_daemon_accepts_spawns_bind_and_data_dir() {
    let Command::Run(options) = parse(args(&[
        "--port",
        "9000",
        "--bind",
        "100.64.0.1",
        "--spawn",
        "opencode --model x",
        "--spawn",
        "claude",
        "--data-dir",
        "C:/tmp/hn",
    ]))
    .expect("parse") else {
        panic!("expected run");
    };
    assert_eq!(options.port, 9000);
    assert_eq!(options.bind.to_string(), "100.64.0.1");
    assert_eq!(options.spawns.len(), 2);
    assert!(options.data_dir.ends_with("hn"));
}

#[test]
fn wildcard_binds_are_refused() {
    let error = parse(args(&["--bind", "0.0.0.0"])).unwrap_err();
    assert!(error.contains("wildcard"), "{error}");
    assert!(parse(args(&["--bind", "::"])).is_err());
}

#[test]
fn pair_code_and_devices_subcommands_parse() {
    let Command::PairCode { ttl_seconds, .. } =
        parse(args(&["pair-code", "--ttl", "30"])).expect("parse")
    else {
        panic!("expected pair-code");
    };
    assert_eq!(ttl_seconds, 30.0);

    let Command::Devices { command, .. } =
        parse(args(&["devices", "revoke", "dev-abc"])).expect("parse")
    else {
        panic!("expected devices");
    };
    let DevicesCommand::Revoke { device_id } = command else {
        panic!("expected revoke");
    };
    assert_eq!(device_id, "dev-abc");

    assert!(parse(args(&["devices", "grant", "dev-abc"])).is_err());
    let Command::Devices { command, .. } =
        parse(args(&["devices", "grant", "dev-abc", "-p", "respond"])).expect("parse")
    else {
        panic!("expected devices");
    };
    let DevicesCommand::Grant { permission, .. } = command else {
        panic!("expected grant");
    };
    assert_eq!(permission, "respond");
}

#[test]
fn tls_bind_is_off_by_default_and_needs_a_concrete_address() {
    let Command::Run(options) = parse(args(&[])).expect("parse") else {
        panic!("expected run");
    };
    assert!(options.tls_bind.is_empty(), "the LAN listener is opt-in");

    let Command::Run(options) = parse(args(&["--tls-bind", "192.168.0.10:8443"])).expect("parse")
    else {
        panic!("expected run");
    };
    assert_eq!(
        options
            .tls_bind
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["192.168.0.10:8443"]
    );

    let Command::Run(options) = parse(args(&[
        "--tls-bind",
        "192.168.0.10:8443",
        "--tls-bind",
        "100.64.0.5:8443",
    ]))
    .expect("parse") else {
        panic!("expected run");
    };
    assert_eq!(
        options
            .tls_bind
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["192.168.0.10:8443", "100.64.0.5:8443"],
        "the flag repeats: LAN and VPN at once"
    );

    let error = parse(args(&["--tls-bind", "0.0.0.0:8443"])).unwrap_err();
    assert!(error.contains("wildcard"), "{error}");
    assert!(parse(args(&["--tls-bind", "[::]:8443"])).is_err());
    assert!(
        parse(args(&["--tls-bind", "192.168.0.10"])).is_err(),
        "a port is required"
    );
}

#[test]
fn pair_code_takes_concrete_lan_addresses() {
    let Command::PairCode { lan, .. } = parse(args(&[
        "pair-code",
        "--lan",
        "192.168.0.10:8443",
        "--lan",
        "100.64.0.1:8443",
    ]))
    .expect("parse") else {
        panic!("expected pair-code");
    };
    assert_eq!(lan.len(), 2);
    assert!(parse(args(&["pair-code", "--lan", "0.0.0.0:8443"])).is_err());
}

#[test]
fn tls_is_a_subcommand_with_prepare_and_activate() {
    let Command::Tls { command, data_dir } =
        parse(args(&["tls", "prepare", "--data-dir", "C:/tmp/hn"])).expect("parse")
    else {
        panic!("expected tls");
    };
    assert!(matches!(command, TlsCommand::Prepare));
    assert!(data_dir.ends_with("hn"));

    let Command::Tls { command, .. } =
        parse(args(&["tls", "activate", "--data-dir", "C:/tmp/hn"])).expect("parse")
    else {
        panic!("expected tls");
    };
    assert!(matches!(command, TlsCommand::Activate));

    assert!(parse(args(&["tls"])).is_err());
    assert!(parse(args(&["tls", "rotate", "--data-dir", "C:/tmp/hn"])).is_err());
}

#[test]
fn the_mail_sidecar_is_off_unless_named_and_only_loopback_is_accepted() {
    let Command::Run(options) = parse(args(&[])).expect("parse") else {
        panic!("expected run");
    };
    assert!(options.mail_sidecar.is_none());

    let Command::Run(options) =
        parse(args(&["--mail-sidecar", "http://localhost:8766"])).expect("parse")
    else {
        panic!("expected run");
    };
    assert!(options.mail_sidecar.is_some());

    for url in [
        "http://10.0.0.5:8766",
        "https://127.0.0.1:8766",
        "http://127.0.0.1:8766/v1/triage",
        "http://user:secret@127.0.0.1:8766",
    ] {
        let error = parse(args(&["--mail-sidecar", url])).unwrap_err();
        assert!(error.contains("--mail-sidecar"), "{error}");
        assert!(!error.contains("secret") && !error.contains(url), "{error}");
    }
    assert!(parse(args(&["--mail-sidecar"])).is_err());
}

#[test]
fn the_speech_engine_is_off_unless_all_three_flags_are_named() {
    let Command::Run(options) = parse(args(&[])).expect("parse") else {
        panic!("expected run");
    };
    assert!(options.asr.is_none());

    let Command::Run(options) = parse(args(&[
        "--asr-python",
        "py/python.exe",
        "--asr-script",
        "py/asr_worker.py",
        "--asr-model",
        "models/canary",
    ]))
    .expect("parse") else {
        panic!("expected run");
    };
    let asr = options.asr.expect("speech is configured");
    assert_eq!(asr.python, PathBuf::from("py/python.exe"));
    assert_eq!(asr.model, PathBuf::from("models/canary"));

    let error = parse(args(&["--asr-python", "py/python.exe"])).unwrap_err();
    assert!(error.contains("go together"), "{error}");
    assert!(parse(args(&["--asr-model"])).is_err());
}
