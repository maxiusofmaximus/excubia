//! The `excubia-host strata` subcommands: start, stop, status and enable-agent.

use std::path::PathBuf;

use super::parse::resolve_data_dir;
use super::types::{Command, StrataCommand};

pub(super) fn strata(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let sub = args
        .next()
        .ok_or("strata needs a subcommand: start|stop|status|enable-agent")?;
    let mut data_dir = None;
    let mut voice_stopped = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    args.next().ok_or("--data-dir needs a value")?,
                ))
            }
            "--voz-parada" => voice_stopped = true,
            other => return Err(format!("unknown argument for strata: {other}")),
        }
    }
    // `--voz-parada` is a legacy no-op for `start`: the voice host now yields by itself.
    let command = match sub.as_str() {
        "start" => StrataCommand::Start,
        "stop" | "status" | "enable-agent" if voice_stopped => {
            return Err("--voz-parada only applies to strata start".to_string())
        }
        "stop" => StrataCommand::Stop,
        "status" => StrataCommand::Status,
        "enable-agent" => StrataCommand::EnableAgent,
        other => return Err(format!("unknown strata subcommand: {other}")),
    };
    Ok(Command::Strata {
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

    #[test]
    fn start_accepts_the_legacy_voice_flag_and_nothing_else_does() {
        let Command::Strata { command, .. } =
            strata(args(&["start", "--voz-parada", "--data-dir", "C:/tmp/hn"])).expect("parse")
        else {
            panic!("expected strata");
        };
        assert_eq!(command, StrataCommand::Start);
        assert!(strata(args(&["start"])).is_ok());

        assert!(strata(args(&["stop", "--voz-parada"])).is_err());
        assert!(strata(args(&["status", "--otra"])).is_err());
        assert!(strata(args(&["arranca"])).is_err());
        assert!(strata(args(&[])).is_err());
    }
}
