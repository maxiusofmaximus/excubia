//! Synthetic signals for tests: a sine tone and deterministic noise.

use crate::limits::SAMPLE_RATE;

pub fn tone(seconds: f32, amplitude: f32) -> Vec<f32> {
    let rate = SAMPLE_RATE as f32;
    (0..(seconds * rate) as usize)
        .map(|n| amplitude * (n as f32 * 2.0 * std::f32::consts::PI * 220.0 / rate).sin())
        .collect()
}

/// Deterministic noise in [-amplitude, amplitude]; no RNG dependency for a test.
pub fn noise(seconds: f32, amplitude: f32) -> Vec<f32> {
    let mut state: u32 = 0x1234_5678;
    (0..(seconds * SAMPLE_RATE as f32) as usize)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            amplitude * (state as f32 / u32::MAX as f32 * 2.0 - 1.0)
        })
        .collect()
}
