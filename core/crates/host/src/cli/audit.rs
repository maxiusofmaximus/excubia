//! The `excubia-host audit` subcommand: `tail` prints the last events of the audit log.

use std::path::PathBuf;

use super::parse::resolve_data_dir;
use super::types::Command;

const DEFAULT_TAIL: usize = 50;

pub(super) fn audit(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    match args.next().as_deref() {
        Some("tail") => {}
        _ => return Err("audit needs a subcommand: tail".to_string()),
    }
    let mut n = DEFAULT_TAIL;
    let mut data_dir = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--n" => {
                let value = args.next().ok_or("--n needs a value")?;
                n = value
                    .parse()
                    .map_err(|_| format!("--n needs a whole number, not {value}"))?;
            }
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    args.next().ok_or("--data-dir needs a value")?,
                ))
            }
            other => return Err(format!("unknown argument for audit tail: {other}")),
        }
    }
    Ok(Command::AuditTail {
        n,
        data_dir: resolve_data_dir(data_dir)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> impl Iterator<Item = String> {
        items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn tail_takes_a_count_and_a_data_dir_and_nothing_else() {
        match audit(args(&["tail", "--n", "5", "--data-dir", "C:/tmp/hn"])) {
            Ok(Command::AuditTail { n, .. }) => assert_eq!(n, 5),
            other => panic!("expected audit tail, got {other:?}"),
        }
        match audit(args(&["tail"])) {
            Ok(Command::AuditTail { n, .. }) => assert_eq!(n, DEFAULT_TAIL),
            other => panic!("expected audit tail, got {other:?}"),
        }
        assert!(audit(args(&[])).is_err());
        assert!(audit(args(&["head"])).is_err());
        assert!(audit(args(&["tail", "--n", "x"])).is_err());
        assert!(audit(args(&["tail", "--otra"])).is_err());
    }
}
