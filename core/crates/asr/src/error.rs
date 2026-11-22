use thiserror::Error;

/// Why a transcription or an audio decode failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AsrError {
    #[error("the speech engine needs CUDA, which is not available")]
    NoCuda,
    #[error("the speech engine could not start")]
    WorkerStart,
    #[error("the speech engine exited")]
    WorkerExited,
    #[error("the speech engine did not answer in time")]
    Timeout,
    #[error("the speech engine sent a line that is not protocol")]
    InvalidReply,
    #[error("the speech engine reported an error: {0}")]
    Engine(String),
    #[error("the audio is not a valid Ogg Opus stream")]
    InvalidAudio,
    #[error("the audio is not a WAV file of 16-bit mono PCM at 16 kHz")]
    UnsupportedAudio,
    #[error("the audio is longer than the allowed duration")]
    AudioTooLong,
    #[error("the audio is larger than the allowed size")]
    AudioTooLarge,
}
