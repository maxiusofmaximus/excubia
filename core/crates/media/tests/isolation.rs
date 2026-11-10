mod support;

use std::time::Duration;

use excubia_core_media::{process_image, process_image_isolated, Limits, MediaError};
use image::ImageFormat;
use support::{encode, gradient};

const GENEROUS: Duration = Duration::from_secs(60);

#[test]
fn worker_returns_the_same_bytes_as_in_process() {
    let png = encode(&gradient(64, 48), ImageFormat::Png);
    let isolated = process_image_isolated(&png, Limits::default(), GENEROUS)
        .expect("worker processes a valid PNG");
    let in_process = process_image(&png, Limits::default()).unwrap();
    assert_eq!(isolated, in_process);
}

#[test]
fn worker_reports_refusals_as_codes() {
    let gif = b"GIF89a\x01\x00\x01\x00\x80\x00\x00".to_vec();
    assert_eq!(
        process_image_isolated(&gif, Limits::default(), GENEROUS).unwrap_err(),
        MediaError::UnsupportedType
    );
}

#[test]
fn one_millisecond_timeout_kills_the_worker() {
    let png = encode(&gradient(64, 48), ImageFormat::Png);
    assert_eq!(
        process_image_isolated(&png, Limits::default(), Duration::from_millis(1)).unwrap_err(),
        MediaError::Timeout
    );
}
