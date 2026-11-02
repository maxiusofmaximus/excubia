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
