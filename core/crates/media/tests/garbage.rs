mod support;

use excubia_core_media::{process_image, Limits};
use image::ImageFormat;
use support::{encode, gradient};

/// xorshift64: a fixed seed keeps the run reproducible.
fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

#[test]
fn random_bytes_never_panic() {
    for seed in [1, 2, 3, 0xDEAD_BEEF, 0x1234_5678_9ABC_DEF0] {
        for len in [0, 1, 7, 8, 64, 4096] {
            let bytes = noise(seed, len);
            let _ = process_image(&bytes, Limits::default());
        }
    }
}

#[test]
fn valid_headers_over_random_bodies_are_refused() {
    let png = encode(&gradient(16, 16), ImageFormat::Png);
    let jpeg = encode(&gradient(16, 16), ImageFormat::Jpeg);
    let webp = encode(&gradient(16, 16), ImageFormat::WebP);
    for valid in [&png, &jpeg, &webp] {
        for seed in [7, 11, 13] {
            let mut bytes = valid[..12].to_vec();
            bytes.extend(noise(seed, 2048));
            assert!(process_image(&bytes, Limits::default()).is_err());
        }
    }
}
