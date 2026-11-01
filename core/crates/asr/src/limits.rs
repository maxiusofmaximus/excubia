use std::time::Duration;

/// Canary input rate; the VAD and the worker both assume it.
pub const SAMPLE_RATE: u32 = 16_000;
/// Longest decoded audio accepted from an Ogg Opus stream.
pub const MAX_AUDIO_SECONDS: usize = 120;
pub(crate) const MAX_AUDIO_SAMPLES: usize = MAX_AUDIO_SECONDS * SAMPLE_RATE as usize;
/// Largest request the worker accepts (`MAX_SAMPLES` in `asr_worker.py`); a bigger header makes it exit.
pub(crate) const MAX_REQUEST_SAMPLES: usize = 30 * SAMPLE_RATE as usize;
/// Opus at 510 kbit/s for the full 120 s, with headroom; larger input is not speech from a phone.
pub(crate) const MAX_OPUS_BYTES: usize = 8 * 1024 * 1024;
/// A 20 s chunk takes about 2 s on the spiked GPU; the rest is headroom for a busy card.
pub(crate) const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Model load measured at 7 s warm and 134 s cold (Qwen3-ASR from a cold disk); the first
/// start after a boot needs the headroom.
pub(crate) const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(300);
/// One reply is a short JSON line; this caps a worker that never sends a newline.
pub(crate) const MAX_REPLY_LINE_BYTES: u64 = 1024 * 1024;
