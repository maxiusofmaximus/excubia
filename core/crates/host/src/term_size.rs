//! The terminal geometry an agent's pty and screen are built with.
//!
//! `vt100` only accepts non-zero dimensions, and the pty layer takes plain
//! `u16`s. Checking once here means a zero size is refused before a process
//! is spawned, and everything downstream gets the non-zero types directly.

use std::num::NonZeroU16;

use hn_core_pty::PtySize;
use thiserror::Error;

/// A terminal size with at least one row and one column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TermSize {
    rows: NonZeroU16,
    cols: NonZeroU16,
}

/// The rejected size: at least one dimension was zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("terminal size must be at least 1x1, got {rows}x{cols}")]
pub struct ZeroDimension {
    rows: u16,
    cols: u16,
}

impl TermSize {
    pub fn rows(self) -> NonZeroU16 {
        self.rows
    }

    pub fn cols(self) -> NonZeroU16 {
        self.cols
    }
}

impl TryFrom<PtySize> for TermSize {
    type Error = ZeroDimension;

    fn try_from(size: PtySize) -> Result<Self, Self::Error> {
        match (NonZeroU16::new(size.rows), NonZeroU16::new(size.cols)) {
            (Some(rows), Some(cols)) => Ok(Self { rows, cols }),
            _ => Err(ZeroDimension {
                rows: size.rows,
                cols: size.cols,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(rows: u16, cols: u16) -> PtySize {
        PtySize { rows, cols }
    }

    #[test]
    fn any_zero_dimension_is_rejected() {
        // A resize has to go through this same conversion, so a zero size is
        // refused here instead of reaching the parser.
        for (rows, cols) in [(0, 80), (24, 0), (0, 0)] {
            assert_eq!(
                TermSize::try_from(size(rows, cols)),
                Err(ZeroDimension { rows, cols })
            );
        }
    }

    #[test]
    fn one_by_one_is_the_smallest_accepted_size() {
        let term = TermSize::try_from(size(1, 1)).expect("1x1 is valid");
        assert_eq!((term.rows().get(), term.cols().get()), (1, 1));
    }

    #[test]
    fn one_by_one_parser_keeps_working() {
        let term = TermSize::try_from(size(1, 1)).expect("1x1 is valid");
        let mut parser = vt100::Parser::new(term.rows(), term.cols(), 0);
        parser.process(b"x");
        assert_eq!(parser.screen().contents(), "x");
    }

    #[test]
    fn default_pty_size_is_accepted() {
        assert!(TermSize::try_from(PtySize::default()).is_ok());
    }
}
