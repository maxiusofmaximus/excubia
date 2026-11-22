//! Ogg Opus to 16 kHz mono f32. Opus decodes to 16 kHz natively, so no resampler is needed.

use std::io::Cursor;

use ogg::PacketReader;
use opus_rs::OpusDecoder;

use crate::error::AsrError;
use crate::limits::{MAX_AUDIO_SAMPLES, MAX_OPUS_BYTES, SAMPLE_RATE};

/// Opus `pre_skip` and the Ogg Opus header count in 48 kHz samples, whatever the decode rate.
const HEADER_RATE: u64 = 48_000;
const OPUS_HEAD_LEN: usize = 19;
/// Opus packets carry at most 120 ms.
const MAX_FRAME_SAMPLES: usize = 120 * SAMPLE_RATE as usize / 1000;

pub fn decode_ogg_opus(bytes: &[u8]) -> Result<Vec<f32>, AsrError> {
    if bytes.len() > MAX_OPUS_BYTES {
        return Err(AsrError::AudioTooLarge);
    }
    let mut reader = PacketReader::new(Cursor::new(bytes));
    let head = reader
        .read_packet()
        .map_err(invalid)?
        .ok_or(AsrError::InvalidAudio)?;
    let (channels, pre_skip) = parse_head(&head.data)?;
    let tags = reader
        .read_packet()
        .map_err(invalid)?
        .ok_or(AsrError::InvalidAudio)?;
    if !tags.data.starts_with(b"OpusTags") {
        return Err(AsrError::InvalidAudio);
    }
    let mut decoder = OpusDecoder::new(SAMPLE_RATE as i32, channels).map_err(invalid)?;
    let mut frame = vec![0.0; MAX_FRAME_SAMPLES * channels];
    let mut skip = pre_skip * SAMPLE_RATE as usize / HEADER_RATE as usize;
    let mut mono = Vec::new();
    while let Some(packet) = reader.read_packet().map_err(invalid)? {
        let per_channel = decoder
            .decode(&packet.data, MAX_FRAME_SAMPLES, &mut frame)
            .map_err(invalid)?;
        append_mono(
            &mut mono,
            &frame[..per_channel * channels],
            channels,
            &mut skip,
        );
        if mono.len() > MAX_AUDIO_SAMPLES {
            return Err(AsrError::AudioTooLong);
        }
    }
    Ok(mono)
}

/// Channel count and pre-skip from `OpusHead`. Only mono and stereo, mapping family 0.
fn parse_head(data: &[u8]) -> Result<(usize, usize), AsrError> {
    if data.len() < OPUS_HEAD_LEN || !data.starts_with(b"OpusHead") || data[18] != 0 {
        return Err(AsrError::InvalidAudio);
    }
    let channels = usize::from(data[9]);
    if !(1..=2).contains(&channels) {
        return Err(AsrError::InvalidAudio);
    }
    Ok((
        channels,
        usize::from(u16::from_le_bytes([data[10], data[11]])),
    ))
}

/// Downmixes interleaved frames to mono and drops the leading `skip` frames.
fn append_mono(mono: &mut Vec<f32>, interleaved: &[f32], channels: usize, skip: &mut usize) {
    for frame in interleaved.chunks_exact(channels) {
        if *skip > 0 {
            *skip -= 1;
            continue;
        }
        mono.push(frame.iter().sum::<f32>() / channels as f32);
    }
}

fn invalid<E>(_: E) -> AsrError {
    AsrError::InvalidAudio
}

#[cfg(test)]
mod tests {
    use super::{decode_ogg_opus, MAX_OPUS_BYTES};
    use crate::error::AsrError;
    use crate::limits::SAMPLE_RATE;

    const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/tones_2s5.opus");

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn decodes_the_fixture_to_its_two_and_a_half_seconds() {
        let pcm = decode_ogg_opus(FIXTURE).unwrap();
        let expected = 5 * SAMPLE_RATE as usize / 2;
        assert!(
            pcm.len().abs_diff(expected) <= 320,
            "got {} samples",
            pcm.len()
        );
        // Tones (0.3 amplitude) at 0–1 s and 1.5–2.5 s; silence at 1.1–1.4 s.
        assert!(rms(&pcm[..16_000]) > 0.1);
        assert!(rms(&pcm[17_600..22_400]) < 0.01);
        assert!(rms(&pcm[pcm.len() - 16_000..]) > 0.1);
    }

    #[test]
    fn truncated_and_corrupted_streams_fail_without_panicking() {
        let step = FIXTURE.len() / 64;
        for length in (0..FIXTURE.len()).step_by(step) {
            let _ = decode_ogg_opus(&FIXTURE[..length]);
        }
        let mut state: u32 = 0x9e37_79b9;
        for _ in 0..64 {
            let mut bytes = FIXTURE.to_vec();
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let at = state as usize % bytes.len();
            bytes[at] ^= (state >> 16) as u8 | 1;
            let _ = decode_ogg_opus(&bytes);
        }
    }

    #[test]
    fn oversized_input_is_refused_before_decoding() {
        let bytes = vec![0_u8; MAX_OPUS_BYTES + 1];
        assert_eq!(decode_ogg_opus(&bytes), Err(AsrError::AudioTooLarge));
    }
}
