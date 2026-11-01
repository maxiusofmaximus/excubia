use std::collections::HashSet;

use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};
use webp::Encoder;

/// Lossy quality for photos; screenshots and text stay lossless.
const LOSSY_QUALITY: f32 = 85.0;
/// Images with at most this many distinct colors (in a sample) are treated as screenshots.
const MAX_PALETTE_COLORS: usize = 256;
const SAMPLE_PIXELS: usize = 65_536;

/// Scales the image so its longest side is at most `max_side`, keeping the aspect ratio.
pub(crate) fn fit_long_side(image: DynamicImage, max_side: u32) -> DynamicImage {
    if image.width().max(image.height()) <= max_side {
        image
    } else {
        image.resize(max_side, max_side, FilterType::Lanczos3)
    }
}

/// WebP lossless for PNG input or few-color images, lossy q85 for the rest.
/// The encoder writes pixels only, so no metadata survives.
pub(crate) fn encode_webp(image: &DynamicImage, from_png: bool) -> Vec<u8> {
    let rgba = image.to_rgba8();
    let encoder = Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    if from_png || has_few_colors(&rgba) {
        encoder.encode_lossless().to_vec()
    } else {
        encoder.encode(LOSSY_QUALITY).to_vec()
    }
}

fn has_few_colors(rgba: &RgbaImage) -> bool {
    let step = (rgba.pixels().len() / SAMPLE_PIXELS).max(1);
    let mut colors = HashSet::new();
    for pixel in rgba.pixels().step_by(step) {
        colors.insert(pixel.0);
        if colors.len() > MAX_PALETTE_COLORS {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};

    use super::has_few_colors;

    #[test]
    fn flat_colors_are_few_and_gradients_are_not() {
        let flat = RgbaImage::from_pixel(64, 64, Rgba([10, 20, 30, 255]));
        assert!(has_few_colors(&flat));
        let gradient = RgbaImage::from_fn(64, 64, |x, y| Rgba([x as u8 * 4, y as u8 * 4, 0, 255]));
        assert!(!has_few_colors(&gradient));
    }
}
