//! Reads one image on stdin, writes the reply frame on stdout. Arguments: the four `Limits` values.

use std::io;
use std::process::ExitCode;

use hn_core_media::worker::{parse_limits, serve};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(limits) = parse_limits(&args) else {
        return ExitCode::from(2);
    };
    match serve(io::stdin().lock(), io::stdout().lock(), limits) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
