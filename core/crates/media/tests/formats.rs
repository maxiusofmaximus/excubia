mod support;

use excubia_media::{process_image, Limits, MediaError};
use image::ImageFormat;
use support::{encode, flat, gradient};

fn assert_webp(input: &[u8], width: u32, height: u32) {
    let out = process_image(input, Limits::default()).expect("valid image is accepted");
    assert_eq!(out.mime, "image/webp");
    assert_eq!((out.width, out.height), (width, height));
    assert_eq!(&out.bytes[0..4], b"RIFF");
    assert_eq!(&out.bytes[8..12], b"WEBP");
    let decoded = image::load_from_memory_with_format(&out.bytes, ImageFormat::WebP)
        .expect("output decodes as WebP");
    assert_eq!((decoded.width(), decoded.height()), (width, height));
}

#[test]
fn png_jpeg_and_webp_come_out_as_webp() {
    assert_webp(&encode(&flat(120, 80), ImageFormat::Png), 120, 80);
    assert_webp(&encode(&gradient(120, 80), ImageFormat::Jpeg), 120, 80);
    assert_webp(&encode(&gradient(120, 80), ImageFormat::WebP), 120, 80);
}

#[test]
fn text_named_png_is_refused() {
    let text = b"this is not an image, only text named like one".to_vec();
    assert_eq!(
        process_image(&text, Limits::default()).unwrap_err(),
        MediaError::UnsupportedType
    );
}

#[test]
fn png_signature_over_garbage_is_invalid_not_accepted() {
    let mut exe = b"\x89PNG\r\n\x1a\n".to_vec();
    exe.extend_from_slice(b"MZ\x90\x00\x03\x00\x00\x00 truncated executable");
    assert_eq!(
        process_image(&exe, Limits::default()).unwrap_err(),
        MediaError::InvalidImage
    );
}

#[test]
fn gif_svg_and_pdf_are_unsupported() {
    let inputs: [&[u8]; 3] = [
        b"GIF89a\x01\x00\x01\x00\x80\x00\x00",
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script/></svg>",
        b"%PDF-1.7\n1 0 obj\n<< >>\nendobj\n",
    ];
    for input in inputs {
        assert_eq!(
            process_image(input, Limits::default()).unwrap_err(),
            MediaError::UnsupportedType
        );
    }
}
