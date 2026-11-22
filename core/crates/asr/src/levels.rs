//! Levels of decoded audio for the host log. Numbers only: never the samples, never the text.
//! The speech figures come from the same segmentation the engine uses.

use crate::limits::SAMPLE_RATE;
use crate::vad;

/// A sample this close to full scale counts as clipped.
const CLIP_LEVEL: f32 = 0.99;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioLevels {
    pub seconds: f32,
    pub peak_dbfs: f32,
    pub rms_dbfs: f32,
    pub clipped_percent: f32,
    pub speech_seconds: f32,
    pub chunks: usize,
    pub noise_floor_dbfs: f32,
    pub threshold_dbfs: f32,
}

/// Empty audio is silence (`-inf` dBFS, no speech); it never panics.
pub fn audio_levels(pcm: &[f32]) -> AudioLevels {
    let segmentation = vad::segment(pcm);
    let speech_samples: usize = segmentation.chunks.iter().map(|range| range.len()).sum();
    let peak = pcm
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    let clipped = pcm
        .iter()
        .filter(|sample| sample.abs() >= CLIP_LEVEL)
        .count();
    let samples = pcm.len().max(1) as f32;
    AudioLevels {
        seconds: pcm.len() as f32 / SAMPLE_RATE as f32,
        peak_dbfs: 20.0 * peak.log10(),
        rms_dbfs: vad::level_dbfs(pcm),
        clipped_percent: 100.0 * clipped as f32 / samples,
        speech_seconds: speech_samples as f32 / SAMPLE_RATE as f32,
        chunks: segmentation.chunks.len(),
        noise_floor_dbfs: segmentation.noise_floor_dbfs,
        threshold_dbfs: segmentation.threshold_dbfs,
    }
}

#[cfg(test)]
mod tests {
    use super::audio_levels;
    use crate::limits::SAMPLE_RATE;
    use crate::signal::tone;

    #[test]
    fn tone_levels_match_its_amplitude() {
        // A sine of amplitude A has peak A and rms A / sqrt(2).
        let levels = audio_levels(&tone(1.0, 0.3));
        let peak_dbfs = 20.0 * 0.3_f32.log10();
        let rms_dbfs = 20.0 * (0.3_f32 / 2.0_f32.sqrt()).log10();
        assert!((levels.peak_dbfs - peak_dbfs).abs() < 0.5, "{levels:?}");
        assert!((levels.rms_dbfs - rms_dbfs).abs() < 0.5, "{levels:?}");
    }

    #[test]
    fn a_saturated_signal_is_clipped() {
        assert!(audio_levels(&tone(1.0, 1.0)).clipped_percent > 0.0);
        assert_eq!(audio_levels(&tone(1.0, 0.3)).clipped_percent, 0.0);
    }

    #[test]
    fn silence_has_no_speech() {
        let levels = audio_levels(&[0.0; SAMPLE_RATE as usize]);
        assert_eq!(levels.chunks, 0);
        assert_eq!(levels.speech_seconds, 0.0);
    }

    #[test]
    fn empty_audio_is_silence_not_a_panic() {
        let levels = audio_levels(&[]);
        assert_eq!(levels.seconds, 0.0);
        assert_eq!(levels.chunks, 0);
        assert_eq!(levels.clipped_percent, 0.0);
        assert_eq!(levels.peak_dbfs, f32::NEG_INFINITY);
        assert_eq!(levels.rms_dbfs, f32::NEG_INFINITY);
    }
}
