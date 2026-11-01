//! Needs CUDA and the Whisper large-v3 snapshot (faster-whisper); the venv needs faster-whisper 1.2.1
//! and the nvidia-* CUDA wheels. Run once with:
//! `EXCUBIA_ASR_PYTHON=<venv python> EXCUBIA_ASR_MODEL=<snapshot dir> EXCUBIA_ASR_AUDIO=<16 kHz mono wav>`
//! `cargo test -p hn-core-asr --test whisper -- --ignored --nocapture`

mod common;

use std::path::PathBuf;

use common::{env_path, read_wav_16k_mono};
use hn_core_asr::{AsrConfig, AsrEngine};

fn whisper_config() -> AsrConfig {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("worker")
        .join("asr_worker_whisper.py");
    AsrConfig::new(
        env_path("EXCUBIA_ASR_PYTHON"),
        script,
        env_path("EXCUBIA_ASR_MODEL"),
    )
}

#[test]
#[ignore = "needs CUDA and the Whisper model; see the module docs"]
fn whisper_transcribes_the_clip() {
    let pcm = read_wav_16k_mono(&env_path("EXCUBIA_ASR_AUDIO"));
    // spawn returns only after the ready line, which the worker sends once CUDA is confirmed.
    let mut engine = AsrEngine::spawn(whisper_config()).expect("the worker starts on CUDA");
    let text = engine.transcribe_audio(&pcm).expect("the clip transcribes");
    eprintln!("text: {text}");
    assert!(!text.is_empty());
}

#[test]
#[ignore = "needs CUDA and the Whisper model; see the module docs"]
fn whisper_hears_silence_as_nothing() {
    // The worker drops the phrases Whisper invents for silence, so none reaches the host.
    let mut engine = AsrEngine::spawn(whisper_config()).expect("the worker starts on CUDA");
    assert_eq!(engine.transcribe_pcm(&[0.0; 16_000]), Ok(String::new()));
}
