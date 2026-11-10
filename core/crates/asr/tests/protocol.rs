//! The client against `fixtures/fake_worker.py`, which fails in the mode named by the model argument.

use std::path::PathBuf;
use std::time::Duration;

use excubia_core_asr::{AsrConfig, AsrEngine, AsrError};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn config(mode: &str) -> AsrConfig {
    let python = std::env::var("EXCUBIA_ASR_PYTHON").unwrap_or_else(|_| "python".to_owned());
    AsrConfig::new(python, fixture("fake_worker.py"), mode)
}

fn engine(mode: &str) -> AsrEngine {
    AsrEngine::spawn(config(mode)).expect("the fake worker starts")
}

#[test]
fn each_request_gets_its_own_text() {
    let mut engine = engine("ok");
    assert_eq!(engine.transcribe_pcm(&[0.0; 160]).unwrap(), "muestras 160");
    assert_eq!(
        engine.transcribe_pcm(&[0.0; 3200]).unwrap(),
        "muestras 3200"
    );
}

#[test]
fn silence_never_reaches_the_worker() {
    // The worker dies on its first request, so any call to it would fail.
    let mut engine = engine("die");
    assert_eq!(engine.transcribe_audio(&[0.0; 16_000]), Ok(String::new()));
}

#[test]
fn speech_chunks_are_joined_with_a_space() {
    let mut pcm = vec![0.3_f32; 16_000];
    pcm.resize(32_000, 0.0);
    pcm.resize(48_000, 0.3);
    let text = engine("ok").transcribe_audio(&pcm).unwrap();
    assert_eq!(text.matches("muestras").count(), 2, "{text}");
}

#[test]
fn an_empty_or_oversized_request_never_reaches_the_worker() {
    // The worker dies on its first request, so any call to it would fail.
    let mut engine = engine("die");
    assert_eq!(engine.transcribe_pcm(&[]), Ok(String::new()));
    let too_long = vec![0.0_f32; 30 * 16_000 + 1];
    assert_eq!(
        engine.transcribe_pcm(&too_long),
        Err(AsrError::AudioTooLong)
    );
}

#[test]
fn an_engine_error_keeps_the_worker() {
    let mut engine = engine("error");
    assert_eq!(
        engine.transcribe_pcm(&[0.0; 16]),
        Err(AsrError::Engine("inference_failed".into()))
    );
    assert_eq!(
        engine.transcribe_pcm(&[0.0; 16]),
        Err(AsrError::Engine("inference_failed".into()))
    );
}

#[test]
fn an_invalid_line_kills_the_worker() {
    let mut engine = engine("garbage");
    assert_eq!(
        engine.transcribe_pcm(&[0.0; 16]),
        Err(AsrError::InvalidReply)
    );
    assert_eq!(
        engine.transcribe_pcm(&[0.0; 16]),
        Err(AsrError::WorkerExited)
    );
}

#[test]
fn a_dead_worker_is_an_error_and_stays_dead() {
    let mut engine = engine("die");
    assert_eq!(
        engine.transcribe_pcm(&[0.0; 16]),
        Err(AsrError::WorkerExited)
    );
    assert_eq!(
        engine.transcribe_pcm(&[0.0; 16]),
        Err(AsrError::WorkerExited)
    );
}

#[test]
fn a_hung_worker_times_out_and_stays_dead() {
    let mut config = config("hang");
    config.request_timeout = Duration::from_millis(300);
    let mut engine = AsrEngine::spawn(config).expect("the fake worker starts");
    assert_eq!(engine.transcribe_pcm(&[0.0; 16]), Err(AsrError::Timeout));
    assert_eq!(
        engine.transcribe_pcm(&[0.0; 16]),
        Err(AsrError::WorkerExited)
    );
}

#[test]
fn a_worker_without_cuda_is_refused_at_start() {
    let refused = AsrEngine::spawn(config("no_cuda")).err();
    assert_eq!(refused, Some(AsrError::NoCuda));
}
