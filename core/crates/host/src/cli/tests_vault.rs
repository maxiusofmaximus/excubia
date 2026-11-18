//! Parser tests of `excubia-host vault`.

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
fn vault_status_and_finish_take_only_the_data_dir() {
    let Command::Vault { command, data_dir } =
        parse(args(&["vault", "status", "--data-dir", "D:/x"])).expect("parse")
    else {
        panic!("expected vault");
    };
    assert_eq!(command, VaultCommand::Status);
    assert_eq!(data_dir, PathBuf::from("D:/x"));

    let Command::Vault { command, .. } =
        parse(args(&["vault", "finish", "--data-dir", "D:/x"])).expect("parse")
    else {
        panic!("expected vault");
    };
    assert_eq!(command, VaultCommand::Finish);
}

#[test]
fn vault_init_needs_its_flags_spelled_out() {
    let Command::Vault { command, .. } = parse(args(&[
        "vault",
        "init",
        "--no-mouse",
        "--host-stopped",
        "--data-dir",
        "D:/x",
    ]))
    .expect("parse") else {
        panic!("expected vault");
    };
    assert_eq!(
        command,
        VaultCommand::Init {
            no_mouse: true,
            host_stopped: true,
            port: DEFAULT_PORT,
        }
    );

    let Command::Vault { command, .. } = parse(args(&[
        "vault",
        "init",
        "--port",
        "9000",
        "--data-dir",
        "D:/x",
    ]))
    .expect("parse") else {
        panic!("expected vault");
    };
    assert_eq!(
        command,
        VaultCommand::Init {
            no_mouse: false,
            host_stopped: false,
            port: 9000,
        },
        "the flags are not defaulted on: the handler refuses without them"
    );
}

#[test]
fn vault_recover_reads_its_phrase_from_stdin_not_from_arguments() {
    let Command::Vault { command, .. } =
        parse(args(&["vault", "recover", "--data-dir", "D:/x"])).expect("parse")
    else {
        panic!("expected vault");
    };
    assert_eq!(command, VaultCommand::Recover);
    assert!(
        parse(args(&["vault", "recover", "palabra", "--data-dir", "D:/x"])).is_err(),
        "a positional phrase would reach the process list"
    );
}

#[test]
fn vault_refuses_what_it_does_not_know() {
    assert!(parse(args(&["vault"])).is_err());
    assert!(parse(args(&["vault", "unlock"])).is_err());
    assert!(parse(args(&["vault", "status", "--force"])).is_err());
    assert!(parse(args(&["vault", "init", "--port", "nope"])).is_err());
}
