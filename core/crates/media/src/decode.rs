use std::io::Cursor;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

use crate::error::MediaError;
use crate::limits::Limits;
use crate::sniff::Kind;

/// Decodes the pixels with the EXIF orientation applied. The dimensions are checked
/// after the header is read and before the pixel buffer is allocated.
pub(crate) fn decode(input: &[u8], kind: Kind, limits: Limits) -> Result<DynamicImage, MediaError> {
    let mut reader = ImageReader::with_format(Cursor::new(input), format_of(kind));
    reader.limits(limits.image_limits());
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| MediaError::from_image(&error))?;
    let (width, height) = decoder.dimensions();
    limits.check_dimensions(width, height)?;
    let orientation = decoder
        .orientation()
        .map_err(|error| MediaError::from_image(&error))?;
    let mut image =
        DynamicImage::from_decoder(decoder).map_err(|error| MediaError::from_image(&error))?;
    image.apply_orientation(orientation);
    Ok(image)
}

fn format_of(kind: Kind) -> ImageFormat {
    match kind {
        Kind::Png => ImageFormat::Png,
        Kind::Jpeg => ImageFormat::Jpeg,
        Kind::WebP => ImageFormat::WebP,
    }
}
