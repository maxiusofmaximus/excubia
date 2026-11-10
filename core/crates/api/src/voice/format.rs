//! The audio containers `POST /v1/voice/transcribe` takes, and how each one becomes the PCM the
//! engine reads.

use excubia_core_asr::{decode_ogg_opus, decode_wav_pcm16, AsrError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    OggOpus,
    Wav,
}

impl AudioFormat {
    pub fn decode(self, bytes: &[u8]) -> Result<Vec<f32>, AsrError> {
        match self {
            Self::OggOpus => decode_ogg_opus(bytes),
            Self::Wav => decode_wav_pcm16(bytes),
        }
    }
}
