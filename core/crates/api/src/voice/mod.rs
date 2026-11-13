//! Speech to text for `POST /v1/voice/transcribe`. The engine is a worker process the host
//! starts on first use; one transcription runs at a time and a second one is refused, not queued.

mod format;
#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use axum::body::Bytes;
use excubia_asr::{audio_levels, AsrConfig, AsrEngine, AsrError, AudioLevels};

pub use format::AudioFormat;
use tokio::sync::Semaphore;

pub const TRANSCRIBE_PATH: &str = "/v1/voice/transcribe";
/// About 60 to 90 seconds of Opus; checked while the body is read, before any decoding.
pub const MAX_VOICE_BODY_BYTES: usize = 2 * 1024 * 1024;
/// Failed starts in a row before the service stops trying until the host restarts.
const MAX_FAILED_STARTS: u8 = 3;

/// A running engine. `transcribe` blocks, so the service calls it off the async runtime.
pub trait Engine: Send {
    fn transcribe(&mut self, pcm: &[f32]) -> Result<String, AsrError>;
}

/// Starts engines. The host's configuration is the production one; tests supply fakes.
pub trait EngineStarter: Send + Sync {
    fn start(&self) -> Result<Box<dyn Engine>, AsrError>;
}

impl Engine for AsrEngine {
    fn transcribe(&mut self, pcm: &[f32]) -> Result<String, AsrError> {
        self.transcribe_audio(pcm)
    }
}

impl EngineStarter for AsrConfig {
    fn start(&self) -> Result<Box<dyn Engine>, AsrError> {
        Ok(Box::new(AsrEngine::spawn(self.clone())?))
    }
}

/// What a transcription can fail with; the route owns the HTTP answer of each.
#[derive(Debug, PartialEq, Eq)]
pub enum VoiceFailure {
    InvalidAudio,
    UnsupportedAudio,
    AudioTooLong,
    Busy,
    Unavailable,
    Internal,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Transcript {
    pub text: String,
    pub ms: u64,
}

impl From<AsrError> for VoiceFailure {
    fn from(error: AsrError) -> Self {
        match error {
            AsrError::InvalidAudio => Self::InvalidAudio,
            AsrError::UnsupportedAudio => Self::UnsupportedAudio,
            AsrError::AudioTooLong | AsrError::AudioTooLarge => Self::AudioTooLong,
            AsrError::Engine(_) => Self::Internal,
            AsrError::NoCuda
            | AsrError::WorkerStart
            | AsrError::WorkerExited
            | AsrError::Timeout
            | AsrError::InvalidReply => Self::Unavailable,
        }
    }
}

struct Slot {
    engine: Option<Box<dyn Engine>>,
    failed_starts: u8,
}

pub struct VoiceService {
    starter: Option<Arc<dyn EngineStarter>>,
    slot: Mutex<Slot>,
    busy: Arc<Semaphore>,
    /// Outside the slot lock: a transcription holds that lock while it runs, and the state read
    /// must not wait for it. `engine` checks it under the lock, so no start slips past `suspend`.
    suspended: AtomicBool,
}

impl VoiceService {
    /// `None` is a host without the speech configuration: every transcription is unavailable.
    pub fn new(starter: Option<Arc<dyn EngineStarter>>) -> Self {
        Self {
            starter,
            slot: Mutex::new(Slot {
                engine: None,
                failed_starts: 0,
            }),
            busy: Arc::new(Semaphore::new(1)),
            suspended: AtomicBool::new(false),
        }
    }

    /// Releases the engine (its worker process ends) and starts nothing until `resume`. A note
    /// in flight finishes first, because the slot lock is held for it.
    pub fn suspend(&self) {
        self.suspended.store(true, Ordering::SeqCst);
        let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        slot.engine = None;
    }

    /// Lifts `suspend` and loads the engine again in the background.
    pub fn resume(self: Arc<Self>) {
        self.suspended.store(false, Ordering::SeqCst);
        self.warm_up();
    }

    /// `unavailable` without the speech configuration, `suspended` while a local model holds
    /// the machine, else `ready`: the engine then starts on the first note.
    pub fn state(&self) -> &'static str {
        if self.starter.is_none() {
            "unavailable"
        } else if self.suspended.load(Ordering::SeqCst) {
            "suspended"
        } else {
            "ready"
        }
    }

    /// Loads the engine in the background so the first note does not pay for it. A busy
    /// service is left alone: a note in flight starts the engine itself.
    pub fn warm_up(self: Arc<Self>) {
        if self.starter.is_none() {
            return;
        }
        let Ok(permit) = Arc::clone(&self.busy).try_acquire_owned() else {
            return;
        };
        tokio::task::spawn_blocking(move || {
            // Held until the engine is in the slot, so a note never sees a half-started one.
            let _permit = permit;
            let started = Instant::now();
            let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
            if self.suspended.load(Ordering::SeqCst) {
                return;
            }
            match self.engine(&mut slot) {
                Ok(engine) => {
                    slot.engine = Some(engine);
                    let ms = started.elapsed().as_millis();
                    eprintln!("voice: engine ready in {ms} ms");
                }
                Err(_) => eprintln!("voice: engine warm-up failed"),
            }
        });
    }

    pub async fn transcribe(
        self: Arc<Self>,
        audio: Bytes,
        format: AudioFormat,
    ) -> Result<Transcript, VoiceFailure> {
        let permit = Arc::clone(&self.busy)
            .try_acquire_owned()
            .map_err(|_| VoiceFailure::Busy)?;
        let started = Instant::now();
        let size = audio.len();
        let joined = tokio::task::spawn_blocking(move || {
            // The permit is held until the work ends, even if the caller has gone.
            let _permit = permit;
            self.run(&audio, format)
        })
        .await;
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let (outcome, levels) = joined.unwrap_or((Err(VoiceFailure::Internal), None));
        eprintln!("{}", log_line(size, ms, levels.as_ref()));
        Ok(Transcript { text: outcome?, ms })
    }

    /// Runs on a blocking thread; the busy permit guarantees one caller at a time. The levels
    /// are kept when recognition fails too: a note that failed is the one worth measuring.
    fn run(
        &self,
        audio: &[u8],
        format: AudioFormat,
    ) -> (Result<String, VoiceFailure>, Option<AudioLevels>) {
        let pcm = match format.decode(audio) {
            Ok(pcm) => pcm,
            Err(error) => return (Err(error.into()), None),
        };
        let levels = audio_levels(&pcm);
        (self.recognise(&pcm), Some(levels))
    }

    fn recognise(&self, pcm: &[f32]) -> Result<String, VoiceFailure> {
        let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        let mut engine = self.engine(&mut slot)?;
        match engine.transcribe(pcm) {
            Ok(text) => {
                slot.engine = Some(engine);
                Ok(text)
            }
            // A worker that answers in protocol is still healthy; any other failure loses it.
            Err(error @ AsrError::Engine(_)) => {
                slot.engine = Some(engine);
                Err(error.into())
            }
            Err(error) => Err(error.into()),
        }
    }

    /// The live engine, or a fresh start after a lost one. Failed starts in a row are counted.
    fn engine(&self, slot: &mut Slot) -> Result<Box<dyn Engine>, VoiceFailure> {
        if self.suspended.load(Ordering::SeqCst) {
            return Err(VoiceFailure::Unavailable);
        }
        if let Some(engine) = slot.engine.take() {
            return Ok(engine);
        }
        let starter = self.starter.as_ref().ok_or(VoiceFailure::Unavailable)?;
        if slot.failed_starts >= MAX_FAILED_STARTS {
            return Err(VoiceFailure::Unavailable);
        }
        match starter.start() {
            Ok(engine) => {
                slot.failed_starts = 0;
                Ok(engine)
            }
            Err(error) => {
                slot.failed_starts += 1;
                Err(error.into())
            }
        }
    }
}

/// The one log line per note: sizes and timings, then what the audio measured. Never the audio
/// and never the recognised text.
fn log_line(size: usize, ms: u64, levels: Option<&AudioLevels>) -> String {
    let head = format!("voice: {size} bytes, {ms} ms");
    let Some(levels) = levels else {
        return head;
    };
    format!(
        "{head}; {:.1}s audio, pico {:.1} dBFS, rms {:.1} dBFS, recorte {:.1}%, voz {:.1}s en {} tramos, suelo {:.1} dBFS, umbral {:.1} dBFS",
        levels.seconds,
        levels.peak_dbfs,
        levels.rms_dbfs,
        levels.clipped_percent,
        levels.speech_seconds,
        levels.chunks,
        levels.noise_floor_dbfs,
        levels.threshold_dbfs,
    )
}
