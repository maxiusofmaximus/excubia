mod support;

use hn_core_media::{process_image, Limits};
use support::jpeg_with_gps_exif;

#[test]
fn exif_gps_and_orientation_are_applied_then_dropped() {
    let input = jpeg_with_gps_exif(40, 20);
    assert!(
        contains(&input, b"Exif\0\0"),
        "the input really carries EXIF"
    );
    let out = process_image(&input, Limits::default()).expect("JPEG with EXIF is accepted");

    // Orientation 6 turns the 40x20 pixels into 20x40 before encoding.
    assert_eq!((out.width, out.height), (20, 40));
    assert!(!contains(&out.bytes, b"Exif\0\0"));
    assert!(!contains(&out.bytes, b"II*\0"));
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
