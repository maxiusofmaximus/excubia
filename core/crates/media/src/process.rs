use sha2::{Digest, Sha256};

use crate::decode::decode;
use crate::encode::{encode_webp, fit_long_side};
use crate::error::MediaError;
use crate::limits::Limits;
use crate::sniff::{sniff, Kind};

pub const WEBP_MIME: &str = "image/webp";

/// Re-encoded image. `bytes` holds pixels only; the caller can trust nothing else from the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessedImage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
}

impl ProcessedImage {
    pub(crate) fn new(bytes: Vec<u8>, width: u32, height: u32) -> Self {
        let sha256 = hex::encode(Sha256::digest(&bytes));
        Self {
            bytes,
            mime: WEBP_MIME,
            width,
            height,
            sha256,
        }
    }
}

/// Validates, decodes in this process and re-encodes to WebP.
/// Use `process_image_isolated` for untrusted input in production.
pub fn process_image(input: &[u8], limits: Limits) -> Result<ProcessedImage, MediaError> {
    if input.len() > limits.max_input_bytes {
        return Err(MediaError::InputTooLarge);
    }
    let kind = sniff(input)?;
    let image = fit_long_side(decode(input, kind, limits)?, limits.max_output_side);
    let (width, height) = (image.width(), image.height());
    let bytes = encode_webp(&image, kind == Kind::Png);
    Ok(ProcessedImage::new(bytes, width, height))
}
