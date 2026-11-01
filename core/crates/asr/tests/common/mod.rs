//! Helpers for the ignored engine tests, which need a GPU and a real model.

use std::path::{Path, PathBuf};

pub fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("set {name}")))
}

/// Reads the 16-bit mono 16 kHz PCM chunk of a WAV file; the spike's TTS exports are that shape.
pub fn read_wav_16k_mono(path: &Path) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("the audio file exists");
    let data = bytes
        .windows(4)
        .position(|four| four == b"data")
        .expect("a data chunk")
        + 8;
    assert_eq!(&bytes[22..24], &1u16.to_le_bytes(), "mono");
    assert_eq!(&bytes[24..28], &16_000u32.to_le_bytes(), "16 kHz");
    bytes[data..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| f32::from(i16::from_le_bytes(*pair)) / 32_768.0)
        .collect()
}
