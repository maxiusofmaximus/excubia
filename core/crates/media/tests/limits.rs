mod support;

use hn_core_media::{process_image, Limits, MediaError};
use image::ImageFormat;
use support::{encode, flat, png_declaring, MIB};

#[test]
fn sides_and_megapixels_are_refused_from_the_header() {
    let side = process_image(&png_declaring(8193, 8193), Limits::default());
    assert_eq!(side.unwrap_err(), MediaError::DimensionsTooLarge);

    let wide = process_image(&png_declaring(8192 * 2, 1), Limits::default());
    assert_eq!(wide.unwrap_err(), MediaError::DimensionsTooLarge);

    let megapixels = process_image(&png_declaring(7000, 6000), Limits::default());
    assert_eq!(megapixels.unwrap_err(), MediaError::ImageTooLarge);
}

#[test]
fn long_side_is_scaled_to_2048_keeping_the_ratio() {
    let big = encode(&flat(4000, 3000), ImageFormat::Jpeg);
    let out = process_image(&big, Limits::default()).expect("4000x3000 is within limits");
    assert_eq!((out.width, out.height), (2048, 1536));
}

#[test]
fn input_over_ten_mib_is_refused_and_exactly_ten_mib_is_only_checked_for_type() {
    let over = vec![0u8; 10 * MIB + 1];
    assert_eq!(
        process_image(&over, Limits::default()).unwrap_err(),
        MediaError::InputTooLarge
    );

    let at_limit = vec![0u8; 10 * MIB];
    assert_eq!(
        process_image(&at_limit, Limits::default()).unwrap_err(),
        MediaError::UnsupportedType
    );
}
