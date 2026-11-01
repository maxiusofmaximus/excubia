//! WAV to 16 kHz mono f32 for the desktop recorder, which cannot encode Ogg Opus. Only the format
//! the engine takes is accepted: nothing is converted or resampled here.

use crate::error::AsrError;
use crate::limits::{MAX_AUDIO_SAMPLES, SAMPLE_RATE};

const RIFF_HEADER_LEN: usize = 12;
const CHUNK_HEADER_LEN: usize = 8;
const FMT_MIN_LEN: usize = 16;
const FORMAT_PCM: u16 = 1;
const CHANNELS_MONO: u16 = 1;
const BITS_PER_SAMPLE: u16 = 16;

pub fn decode_wav_pcm16(bytes: &[u8]) -> Result<Vec<f32>, AsrError> {
    let data = pcm16_mono_16k_data(bytes).ok_or(AsrError::UnsupportedAudio)?;
    if data.is_empty() || !data.len().is_multiple_of(2) {
        return Err(AsrError::UnsupportedAudio);
    }
    if data.len() / 2 > MAX_AUDIO_SAMPLES {
        return Err(AsrError::AudioTooLong);
    }
    Ok(data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| f32::from(i16::from_le_bytes(*pair)) / 32_768.0)
        .collect())
}

/// The `data` bytes of a file whose `fmt ` chunk names the one accepted format.
fn pcm16_mono_16k_data(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut fmt_is_supported = false;
    for (id, body) in chunks(bytes.get(RIFF_HEADER_LEN..)?) {
        match id {
            b"fmt " => fmt_is_supported = is_pcm16_mono_16k(body),
            b"data" => return fmt_is_supported.then_some(body),
            _ => {}
        }
    }
    None
}

/// Each chunk as (id, body), in file order. A streamed `data` (declared size 0 or all ones) runs
/// to the end of the file. A chunk cut short ends the walk.
fn chunks(mut rest: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    std::iter::from_fn(move || {
        let id = rest.get(..4)?;
        let declared = u32::from_le_bytes(rest.get(4..8)?.try_into().ok()?);
        let after_header = rest.get(CHUNK_HEADER_LEN..)?;
        let streamed = id == b"data" && matches!(declared, 0 | u32::MAX);
        let size = if streamed {
            after_header.len()
        } else {
            usize::try_from(declared).ok()?
        };
        let body = after_header.get(..size)?;
        // The pad byte of an odd chunk is optional at the very end of the file.
        rest = after_header.get(size + size % 2..).unwrap_or_default();
        Some((id, body))
    })
}

/// PCM, mono, 16 bits, 16 kHz. The block size and byte rate follow from these and are not checked.
fn is_pcm16_mono_16k(fmt: &[u8]) -> bool {
    fmt.len() >= FMT_MIN_LEN
        && u16::from_le_bytes([fmt[0], fmt[1]]) == FORMAT_PCM
        && u16::from_le_bytes([fmt[2], fmt[3]]) == CHANNELS_MONO
        && u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]) == SAMPLE_RATE
        && u16::from_le_bytes([fmt[14], fmt[15]]) == BITS_PER_SAMPLE
}

#[cfg(test)]
mod tests {
    use super::decode_wav_pcm16;
    use crate::error::AsrError;
    use crate::limits::MAX_AUDIO_SAMPLES;

    fn fmt(format: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
        let block_align = channels * bits / 8;
        let mut body = Vec::new();
        body.extend(format.to_le_bytes());
        body.extend(channels.to_le_bytes());
        body.extend(rate.to_le_bytes());
        body.extend((rate * u32::from(block_align)).to_le_bytes());
        body.extend(block_align.to_le_bytes());
        body.extend(bits.to_le_bytes());
        chunk(b"fmt ", &body)
    }

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        chunk_with(id, body.len() as u32, body)
    }

    /// A chunk whose declared size is not the body length, and a pad byte when the body is odd.
    fn chunk_with(id: &[u8; 4], declared: u32, body: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend(declared.to_le_bytes());
        out.extend(body);
        if body.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    fn pcm(samples: &[i16]) -> Vec<u8> {
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect()
    }

    fn riff(chunks: &[Vec<u8>]) -> Vec<u8> {
        let body = chunks.concat();
        let mut out = b"RIFF".to_vec();
        out.extend(((body.len() + 4) as u32).to_le_bytes());
        out.extend(b"WAVE");
        out.extend(body);
        out
    }

    fn canonical(samples: &[i16]) -> Vec<u8> {
        riff(&[fmt(1, 1, 16_000, 16), chunk(b"data", &pcm(samples))])
    }

    #[test]
    fn a_canonical_file_decodes_to_f32_in_unit_range() {
        let decoded = decode_wav_pcm16(&canonical(&[0, 16_384, -16_384, i16::MIN, i16::MAX]));
        assert_eq!(decoded, Ok(vec![0.0, 0.5, -0.5, -1.0, 32_767.0 / 32_768.0]));
    }

    #[test]
    fn unknown_chunks_between_fmt_and_data_are_skipped() {
        let list = chunk(b"LIST", b"INFOx");
        let file = riff(&[fmt(1, 1, 16_000, 16), list, chunk(b"data", &pcm(&[16_384]))]);
        assert_eq!(decode_wav_pcm16(&file), Ok(vec![0.5]));
    }

    #[test]
    fn a_streamed_data_size_runs_to_the_end_of_the_file() {
        for declared in [u32::MAX, 0] {
            let data = chunk_with(b"data", declared, &pcm(&[16_384, -16_384]));
            let file = riff(&[fmt(1, 1, 16_000, 16), data]);
            assert_eq!(
                decode_wav_pcm16(&file),
                Ok(vec![0.5, -0.5]),
                "declared {declared:#x}"
            );
        }
    }

    #[test]
    fn an_empty_data_chunk_is_unsupported() {
        assert_eq!(
            decode_wav_pcm16(&canonical(&[])),
            Err(AsrError::UnsupportedAudio)
        );
    }

    #[test]
    fn other_formats_are_unsupported() {
        let data = chunk(b"data", &pcm(&[1, 2, 3, 4]));
        let formats = [
            fmt(1, 2, 16_000, 16),
            fmt(1, 1, 16_000, 8),
            fmt(1, 1, 44_100, 16),
            fmt(3, 1, 16_000, 32),
            fmt(0xFFFE, 1, 16_000, 16),
        ];
        for format in formats {
            let file = riff(&[format, data.clone()]);
            assert_eq!(decode_wav_pcm16(&file), Err(AsrError::UnsupportedAudio));
        }
        assert_eq!(
            decode_wav_pcm16(&riff(&[data])),
            Err(AsrError::UnsupportedAudio)
        );
    }

    #[test]
    fn a_truncated_or_odd_file_is_unsupported() {
        let file = canonical(&[1, 2, 3]);
        assert_eq!(
            decode_wav_pcm16(&file[..file.len() - 2]),
            Err(AsrError::UnsupportedAudio)
        );
        assert_eq!(
            decode_wav_pcm16(&file[..20]),
            Err(AsrError::UnsupportedAudio)
        );
        let odd = riff(&[fmt(1, 1, 16_000, 16), chunk(b"data", &[1, 2, 3])]);
        assert_eq!(decode_wav_pcm16(&odd), Err(AsrError::UnsupportedAudio));
    }

    #[test]
    fn bytes_that_are_not_a_wave_file_are_unsupported() {
        assert_eq!(decode_wav_pcm16(&[]), Err(AsrError::UnsupportedAudio));
        assert_eq!(
            decode_wav_pcm16(b"OggS and then some bytes"),
            Err(AsrError::UnsupportedAudio)
        );
    }

    #[test]
    fn more_samples_than_the_duration_limit_is_too_long() {
        let samples = vec![0i16; MAX_AUDIO_SAMPLES + 1];
        assert_eq!(
            decode_wav_pcm16(&canonical(&samples)),
            Err(AsrError::AudioTooLong)
        );
    }
}
