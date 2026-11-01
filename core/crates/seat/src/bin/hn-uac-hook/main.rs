//! `hn-uac-hook`: the blocking hook of the CLIs that support one.
//!
//! A CLI with a pre-tool hook runs this binary before the risky action. It
//! opens an approval in the host, waits for the operator and answers with the
//! exit code the CLI expects:
//!
//! - `0` — the action may run: the operator allowed it and the approval was
//!   consumed just before.
//! - `2` — the action is denied. Covers the operator's refusal, the expired
//!   approval, every way the host can fail to answer (timeout, refused
//!   connection, malformed response) **and** a wrong invocation of the binary.
//!   Fail closed: no answer is no permission, and a CLI must never read the
//!   hook as "broken" and run the action.
//!
//! The credentials come from the environment (`HN_HOOK_URL`, `HN_HOOK_TOKEN`),
//! never from the arguments. The token is the one minted for this run.

mod approval;
mod request;

use std::panic::{self, UnwindSafe};
use std::process::ExitCode;

use approval::{run, Outcome};
use request::Request;

/// The action may run.
const EXIT_ALLOWED: u8 = 0;
/// The action is denied, or the host never gave an answer.
const EXIT_DENIED: u8 = 2;

fn main() -> ExitCode {
    ExitCode::from(guarded(|| exit_code(execute())))
}

/// Runs the hook and turns a panic into a denial. Claude Code blocks only on
/// `2`: a panic that left with `101` would be a non-blocking error, and the
/// action would run.
fn guarded(hook: impl FnOnce() -> u8 + UnwindSafe) -> u8 {
    panic::catch_unwind(hook).unwrap_or(EXIT_DENIED)
}

fn exit_code(result: Result<Outcome, Refusal>) -> u8 {
    match result {
        Ok(Outcome::Allowed) => EXIT_ALLOWED,
        Ok(Outcome::Denied(reason)) | Err(Refusal::NoDecision(reason) | Refusal::Usage(reason)) => {
            eprintln!("hn-uac-hook: {reason}");
            EXIT_DENIED
        }
    }
}

fn execute() -> Result<Outcome, Refusal> {
    let request = Request::read()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| Refusal::NoDecision(error.to_string()))?;
    runtime.block_on(run(&request))
}

/// Why there is no permission. `Usage` is not a decision, but it still exits
/// with `2`: a CLI that saw a different code could treat the hook as broken
/// and run the action.
#[derive(Debug)]
enum Refusal {
    /// The binary was called wrong: an invalid argument. Exit 2.
    Usage(String),
    /// No decision: denied, expired or the host did not answer. Exit 2.
    NoDecision(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_is_a_denial_not_an_unblocking_exit() {
        assert_eq!(guarded(|| panic!("boom")), EXIT_DENIED);
    }

    #[test]
    fn a_normal_answer_passes_through_the_guard() {
        assert_eq!(guarded(|| EXIT_ALLOWED), EXIT_ALLOWED);
        assert_eq!(guarded(|| EXIT_DENIED), EXIT_DENIED);
    }
}
