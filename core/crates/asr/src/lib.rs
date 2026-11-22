//! Local speech-to-text: a Canary worker child process, an energy VAD that feeds it, and Ogg Opus
//! decoding in front of both.

mod corrector;
mod corrector_data;
mod engine;
mod error;
mod levels;
mod lexicon;
mod limits;
mod opus;
#[cfg(test)]
mod signal;
mod vad;
mod wav;
mod wire;

pub use corrector::Corrector;
pub use engine::{AsrConfig, AsrEngine};
pub use error::AsrError;
pub use levels::{audio_levels, AudioLevels};
pub use limits::{MAX_AUDIO_SECONDS, SAMPLE_RATE};
pub use opus::decode_ogg_opus;
pub use wav::decode_wav_pcm16;
