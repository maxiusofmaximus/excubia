//! Reply frame between the worker and its parent. Fixed-width integers and raw bytes only; no serde
//! over data the worker produced from untrusted input.
//!
//! ok:    `0x00` · width u32 BE · height u32 BE · byte length u32 BE · WebP bytes
//! error: `0x01` · code length u8 · code (ASCII, see `MediaError::code`)

use std::io::{self, Write};

use crate::error::MediaError;
use crate::process::ProcessedImage;

const STATUS_OK: u8 = 0x00;
const STATUS_ERR: u8 = 0x01;

pub(crate) fn write_reply(
    output: &mut impl Write,
    result: Result<ProcessedImage, MediaError>,
) -> io::Result<()> {
    match result {
        Ok(image) => {
            let length = u32::try_from(image.bytes.len()).map_err(io::Error::other)?;
            output.write_all(&[STATUS_OK])?;
            output.write_all(&image.width.to_be_bytes())?;
            output.write_all(&image.height.to_be_bytes())?;
            output.write_all(&length.to_be_bytes())?;
            output.write_all(&image.bytes)
        }
        Err(error) => {
            let code = error.code();
            output.write_all(&[STATUS_ERR, code.len() as u8])?;
            output.write_all(code.as_bytes())
        }
    }
}

/// Anything malformed is a worker fault, never an image verdict.
pub(crate) fn parse_reply(frame: &[u8]) -> Result<ProcessedImage, MediaError> {
    match frame.split_first() {
        Some((&STATUS_OK, body)) => parse_ok(body),
        Some((&STATUS_ERR, body)) => parse_err(body),
        _ => Err(MediaError::WorkerFailed),
    }
}

fn parse_ok(body: &[u8]) -> Result<ProcessedImage, MediaError> {
    let width = u32_at(body, 0)?;
    let height = u32_at(body, 4)?;
    let length = u32_at(body, 8)? as usize;
    let bytes = body.get(12..).filter(|bytes| bytes.len() == length);
    bytes
        .map(|bytes| ProcessedImage::new(bytes.to_vec(), width, height))
        .ok_or(MediaError::WorkerFailed)
}

fn parse_err(body: &[u8]) -> Result<ProcessedImage, MediaError> {
    let (&length, code) = body.split_first().ok_or(MediaError::WorkerFailed)?;
    if code.len() != usize::from(length) {
        return Err(MediaError::WorkerFailed);
    }
    let code = std::str::from_utf8(code).map_err(|_| MediaError::WorkerFailed)?;
    Err(MediaError::from_code(code))
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, MediaError> {
    bytes
        .get(at..at + 4)
        .and_then(|four| <[u8; 4]>::try_from(four).ok())
        .map(u32::from_be_bytes)
        .ok_or(MediaError::WorkerFailed)
}

#[cfg(test)]
mod tests {
    use super::{parse_reply, write_reply};
    use crate::error::MediaError;
    use crate::process::ProcessedImage;

    #[test]
    fn ok_frame_round_trips() {
        let image = ProcessedImage::new(b"RIFFxxxxWEBP".to_vec(), 3, 2);
        let mut frame = Vec::new();
        write_reply(&mut frame, Ok(image.clone())).unwrap();
        assert_eq!(parse_reply(&frame), Ok(image));
    }

    #[test]
    fn error_frame_round_trips() {
        let mut frame = Vec::new();
        write_reply(&mut frame, Err(MediaError::InvalidImage)).unwrap();
        assert_eq!(parse_reply(&frame).unwrap_err(), MediaError::InvalidImage);
    }

    #[test]
    fn truncated_frames_are_worker_faults() {
        let mut frame = Vec::new();
        write_reply(&mut frame, Ok(ProcessedImage::new(vec![1, 2, 3], 1, 1))).unwrap();
        frame.pop();
        assert_eq!(parse_reply(&frame), Err(MediaError::WorkerFailed));
        assert_eq!(parse_reply(&[]), Err(MediaError::WorkerFailed));
    }
}
