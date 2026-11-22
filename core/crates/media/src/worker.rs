//! The worker side of the isolation: `excubia-media-worker` runs `serve` on its stdin and stdout.

use std::io::{self, Read, Write};

use crate::limits::Limits;
use crate::process::process_image;
use crate::wire::write_reply;

/// Reads at most one byte over the input limit, so an oversized input is reported, not buffered.
pub fn serve(input: impl Read, mut output: impl Write, limits: Limits) -> io::Result<()> {
    let mut bytes = Vec::new();
    input
        .take(limits.max_input_bytes as u64 + 1)
        .read_to_end(&mut bytes)?;
    let result = process_image(&bytes, limits);
    write_reply(&mut output, result)?;
    output.flush()
}

pub(crate) fn limits_args(limits: Limits) -> [String; 4] {
    [
        limits.max_input_bytes.to_string(),
        limits.max_side.to_string(),
        limits.max_pixels.to_string(),
        limits.max_output_side.to_string(),
    ]
}

/// The worker's command line: the four limits, in the order `limits_args` writes them.
pub fn parse_limits(args: &[String]) -> Option<Limits> {
    let [input, side, pixels, output] = args else {
        return None;
    };
    Some(Limits {
        max_input_bytes: input.parse().ok()?,
        max_side: side.parse().ok()?,
        max_pixels: pixels.parse().ok()?,
        max_output_side: output.parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::{limits_args, parse_limits};
    use crate::limits::Limits;

    #[test]
    fn limits_survive_the_command_line() {
        let limits = Limits::default();
        assert_eq!(parse_limits(&limits_args(limits)), Some(limits));
        assert_eq!(parse_limits(&["1".to_string()]), None);
    }
}
