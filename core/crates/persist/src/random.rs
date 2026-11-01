//! The core's one OS randomness entry point. A failing source is an error for the
//! caller to propagate, never a panic in the middle of a request.

use rand_core::{OsRng, RngCore};
use thiserror::Error;

/// The OS random source failed; nothing was generated. The message carries no data.
#[derive(Debug, Error)]
#[error("the OS random source is unavailable")]
pub struct RandomError;

/// Fill `buffer` with bytes from the OS random source.
pub fn fill_random(buffer: &mut [u8]) -> Result<(), RandomError> {
    OsRng.try_fill_bytes(buffer).map_err(|_| RandomError)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buffer_is_filled_and_repeats_differ() {
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        fill_random(&mut first).expect("first");
        fill_random(&mut second).expect("second");
        assert!(first.iter().any(|byte| *byte != 0));
        assert_ne!(first, second);
    }

    #[test]
    fn an_empty_buffer_is_a_no_op() {
        fill_random(&mut []).expect("empty");
    }
}
