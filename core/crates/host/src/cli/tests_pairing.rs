//! Parser tests for the pairing commands: the default code life and `devices recovery`.

use super::*;
use std::path::PathBuf;

fn args(items: &[&str]) -> impl Iterator<Item = String> {
    items
        .iter()
        .map(|item| item.to_string())
        .collect::<Vec<_>>()
        .into_iter()
}

#[test]
fn a_pairing_code_lives_three_minutes_unless_told_otherwise() {
    let Command::PairCode { ttl_seconds, .. } = parse(args(&["pair-code"])).expect("parse") else {
        panic!("expected pair-code");
    };
    assert_eq!(ttl_seconds, 180.0);
    assert!(USAGE.contains("default 180"), "{USAGE}");
}

#[test]
fn devices_recovery_takes_one_device_id_and_a_data_dir() {
    let Command::Devices { command, data_dir } =
        parse(args(&["devices", "recovery", "dev-abc", "--data-dir", "d"])).expect("parse")
    else {
        panic!("expected devices");
    };
    let DevicesCommand::Recovery { device_id } = command else {
        panic!("expected recovery");
    };
    assert_eq!(device_id, "dev-abc");
    assert_eq!(data_dir, PathBuf::from("d"));
    assert!(USAGE.contains("devices recovery <device_id>"), "{USAGE}");
}

#[test]
fn devices_recovery_needs_a_device_id() {
    assert!(parse(args(&["devices", "recovery"])).is_err());
}

#[test]
fn pair_code_pake_names_the_daemon_port_and_data_dir() {
    let Command::PairCodePake { port, data_dir } = parse(args(&[
        "pair-code",
        "--pake",
        "--port",
        "9000",
        "--data-dir",
        "d",
    ]))
    .expect("parse") else {
        panic!("expected pair-code --pake");
    };
    assert_eq!((port, data_dir), (9000, PathBuf::from("d")));
    let Command::PairCodePake { port, .. } =
        parse(args(&["pair-code", "--pake", "--data-dir", "d"])).expect("parse")
    else {
        panic!("expected pair-code --pake");
    };
    assert_eq!(port, 8791, "the daemon's default port");
    assert!(USAGE.contains("pair-code --pake"), "{USAGE}");
}

#[test]
fn pair_code_pake_refuses_the_flags_it_would_ignore() {
    for extra in [["--ttl", "60"], ["--lan", "192.168.1.5:8443"]] {
        let mut list = vec!["pair-code", "--pake"];
        list.extend(extra);
        assert!(parse(args(&list)).is_err(), "{list:?}");
    }
    assert!(
        parse(args(&["pair-code", "--port", "9000"])).is_err(),
        "--port is only for --pake"
    );
    assert!(parse(args(&["pair-code", "--pake", "--port", "x"])).is_err());
    assert!(parse(args(&["pair-code", "--pake", "--port"])).is_err());
    // The plain command is what it was.
    assert!(matches!(
        parse(args(&["pair-code", "--ttl", "60"])).expect("parse"),
        Command::PairCode { .. }
    ));
}
