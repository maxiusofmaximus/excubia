//! Needs CUDA and the spiked Canary model. Run once with:
//! `EXCUBIA_ASR_PYTHON=<venv python> EXCUBIA_ASR_MODEL=<model dir> EXCUBIA_ASR_AUDIO=<16 kHz mono wav>`
//! `cargo test -p hn-core-asr --test canary -- --ignored --nocapture`

mod common;

use std::path::PathBuf;
use std::time::Instant;

use common::{env_path, read_wav_16k_mono};
use hn_core_asr::{AsrConfig, AsrEngine};

#[test]
#[ignore = "needs CUDA and the Canary model; see the module docs"]
fn canary_transcribes_the_tts_clip() {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("worker")
        .join("asr_worker.py");
    let config = AsrConfig::new(
        env_path("EXCUBIA_ASR_PYTHON"),
        script,
        env_path("EXCUBIA_ASR_MODEL"),
    );
    let pcm = read_wav_16k_mono(&env_path("EXCUBIA_ASR_AUDIO"));
    let mut engine = AsrEngine::spawn(config).expect("the worker starts on CUDA");

    let seconds = pcm.len() as f32 / 16_000.0;
    for run in ["first", "second"] {
        let started = Instant::now();
        let text = engine.transcribe_audio(&pcm).expect("the clip transcribes");
        eprintln!(
            "{run} run: {:?} for {seconds:.1} s of audio",
            started.elapsed()
        );
        eprintln!("text: {text}");
        assert!(!text.is_empty());
    }
}
