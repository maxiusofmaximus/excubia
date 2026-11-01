use crate::error::MediaError;

const MIB: usize = 1024 * 1024;
/// Best-effort cap on decoder allocations; `image` may ignore it, so the worker's timeout backs it up.
const DECODE_ALLOC_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_side: u32,
    pub max_pixels: u64,
    pub max_output_side: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 10 * MIB,
            max_side: 8192,
            max_pixels: 40_000_000,
            max_output_side: 2048,
        }
    }
}

impl Limits {
    pub(crate) fn image_limits(self) -> image::Limits {
        // `image::Limits` is non-exhaustive, so its fields are set one by one.
        let mut limits = image::Limits::no_limits();
        limits.max_image_width = Some(self.max_side);
        limits.max_image_height = Some(self.max_side);
        limits.max_alloc = Some(DECODE_ALLOC_BYTES);
        limits
    }

    /// Checked before any pixel buffer exists.
    pub(crate) fn check_dimensions(self, width: u32, height: u32) -> Result<(), MediaError> {
        if width.max(height) > self.max_side {
            return Err(MediaError::DimensionsTooLarge);
        }
        if u64::from(width) * u64::from(height) > self.max_pixels {
            return Err(MediaError::ImageTooLarge);
        }
        Ok(())
    }
}
