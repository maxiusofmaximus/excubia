//! The `excubia-host alfred` subcommands: local, use local, use claude and status.

use std::path::PathBuf;

use super::parse::resolve_data_dir;
use super::types::{AlfredCommand, Command};

pub(super) fn alfred(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let sub = args
        .next()
        .ok_or("alfred needs a subcommand: local, use local|claude or status")?;
    let command = match sub.as_str() {
        "use" => match args.next().as_deref() {
            Some("local") => AlfredCommand::UseLocal,
            Some("claude") => AlfredCommand::UseClaude,
            _ => return Err("alfred use needs local or claude".to_string()),
        },
        "local" => AlfredCommand::Local,
        "status" => AlfredCommand::Status,
        other => return Err(format!("unknown alfred subcommand: {other}")),
    };
    let mut data_dir = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    args.next().ok_or("--data-dir needs a value")?,
                ))
            }
            other => return Err(format!("unknown argument for alfred: {other}")),
        }
    }
    Ok(Command::Alfred {
        command,
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

    fn command_of(items: &[&str]) -> Result<AlfredCommand, String> {
        match alfred(args(items))? {
            Command::Alfred { command, .. } => Ok(command),
            other => panic!("expected alfred, got {other:?}"),
        }
    }

    #[test]
    fn use_and_status_parse_and_nothing_else_does() {
        assert_eq!(
            command_of(&["use", "local", "--data-dir", "C:/tmp/hn"]),
            Ok(AlfredCommand::UseLocal)
        );
        assert_eq!(command_of(&["use", "claude"]), Ok(AlfredCommand::UseClaude));
        assert_eq!(command_of(&["status"]), Ok(AlfredCommand::Status));

        assert!(alfred(args(&["use"])).is_err());
        assert!(alfred(args(&["use", "gpt"])).is_err());
        assert!(alfred(args(&["status", "--otra"])).is_err());
        assert!(alfred(args(&[])).is_err());
    }

    #[test]
    fn local_parses_with_its_data_dir_and_takes_no_other_argument() {
        assert_eq!(
            command_of(&["local", "--data-dir", "C:/tmp/hn"]),
            Ok(AlfredCommand::Local)
        );
        assert!(alfred(args(&["local", "--otra"])).is_err());
        assert!(alfred(args(&["local", "claude"])).is_err());
    }
}
