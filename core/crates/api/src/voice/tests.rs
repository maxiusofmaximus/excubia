use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use super::*;

const FIXTURE: &[u8] = include_bytes!("../../../asr/tests/fixtures/tones_2s5.opus");

#[derive(Clone, Copy)]
enum Behaviour {
    Answer,
    Lose,
    Refuse,
}

/// Answers with the sample count it was given, so a test sees that the audio was decoded.
struct Echo(Behaviour);

impl Engine for Echo {
    fn transcribe(&mut self, pcm: &[f32]) -> Result<String, AsrError> {
        match self.0 {
            Behaviour::Answer => Ok(format!("{} samples", pcm.len())),
            Behaviour::Lose => Err(AsrError::Timeout),
            Behaviour::Refuse => Err(AsrError::Engine("refused".to_owned())),
        }
    }
}

/// Hands the audio it was given to the test, so the samples themselves can be checked.
struct Capture(Sender<Vec<f32>>);

impl Engine for Capture {
    fn transcribe(&mut self, pcm: &[f32]) -> Result<String, AsrError> {
        let _ = self.0.send(pcm.to_vec());
        Ok("heard".to_owned())
    }
}

/// A canonical 44-byte header: 16 kHz, mono, 16-bit PCM, then the samples.
fn wav(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = b"RIFF".to_vec();
    out.extend((36 + data_len).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(16_000u32.to_le_bytes());
    out.extend(32_000u32.to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data_len.to_le_bytes());
    out.extend(samples.iter().flat_map(|sample| sample.to_le_bytes()));
    out
}

struct Starter {
    behaviour: Behaviour,
    start_fails: bool,
    starts: Arc<AtomicUsize>,
}

impl EngineStarter for Starter {
    fn start(&self) -> Result<Box<dyn Engine>, AsrError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        if self.start_fails {
            return Err(AsrError::WorkerStart);
        }
        Ok(Box::new(Echo(self.behaviour)))
    }
}

/// Blocks inside `transcribe` until the test releases it, so a second request can overlap.
struct Gate {
    entered: Sender<()>,
    release: Receiver<()>,
}

impl Engine for Gate {
    fn transcribe(&mut self, _pcm: &[f32]) -> Result<String, AsrError> {
        let _ = self.entered.send(());
        let _ = self.release.recv();
        Ok("held".to_owned())
    }
}

struct Held(Mutex<Option<Box<dyn Engine>>>);

impl EngineStarter for Held {
    fn start(&self) -> Result<Box<dyn Engine>, AsrError> {
        let mut slot = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        slot.take().ok_or(AsrError::WorkerStart)
    }
}

fn service(behaviour: Behaviour, start_fails: bool) -> (Arc<VoiceService>, Arc<AtomicUsize>) {
    let starts = Arc::new(AtomicUsize::new(0));
    let starter = Starter {
        behaviour,
        start_fails,
        starts: Arc::clone(&starts),
    };
    (Arc::new(VoiceService::new(Some(Arc::new(starter)))), starts)
}

#[tokio::test]
async fn the_fixture_is_decoded_before_the_engine_sees_it() {
    let (service, starts) = service(Behaviour::Answer, false);
    let transcript = service
        .transcribe(Bytes::from_static(FIXTURE), AudioFormat::OggOpus)
        .await
        .expect("the fixture transcribes");
    let samples: usize = transcript
        .text
        .strip_suffix(" samples")
        .and_then(|count| count.parse().ok())
        .expect("the engine answers with a sample count");
    assert!(samples >= 30_000, "2.5 s of audio, got {samples} samples");
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn bytes_that_are_not_ogg_opus_never_reach_the_engine() {
    let (service, starts) = service(Behaviour::Answer, false);
    let failure = service
        .transcribe(
            Bytes::from_static(b"not an ogg stream"),
            AudioFormat::OggOpus,
        )
        .await
        .expect_err("garbage is refused");
    assert_eq!(failure, VoiceFailure::InvalidAudio);
    assert_eq!(starts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_wav_note_reaches_the_engine_as_f32() {
    let (sent, heard) = mpsc::channel();
    let engine: Box<dyn Engine> = Box::new(Capture(sent));
    let service = Arc::new(VoiceService::new(Some(Arc::new(Held(Mutex::new(Some(
        engine,
    )))))));
    let transcript = service
        .transcribe(Bytes::from(wav(&[0, 16_384, -16_384])), AudioFormat::Wav)
        .await
        .expect("the wav transcribes");
    assert_eq!(transcript.text, "heard");
    assert_eq!(
        heard.recv().expect("the engine heard it"),
        vec![0.0, 0.5, -0.5]
    );
}

#[tokio::test]
async fn an_ogg_note_sent_as_wav_is_unsupported_and_never_starts_the_engine() {
    let (service, starts) = service(Behaviour::Answer, false);
    let failure = service
        .transcribe(Bytes::from_static(FIXTURE), AudioFormat::Wav)
        .await
        .expect_err("the fixture is not a wav");
    assert_eq!(failure, VoiceFailure::UnsupportedAudio);
    assert_eq!(starts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_host_without_the_engine_is_unavailable() {
    let service = Arc::new(VoiceService::new(None));
    let failure = service
        .transcribe(Bytes::from_static(FIXTURE), AudioFormat::OggOpus)
        .await
        .expect_err("no engine");
    assert_eq!(failure, VoiceFailure::Unavailable);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_transcription_while_one_runs_is_busy() {
    let (entered, entered_seen) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let gate: Box<dyn Engine> = Box::new(Gate {
        entered,
        release: released,
    });
    let starter = Held(Mutex::new(Some(gate)));
    let service = Arc::new(VoiceService::new(Some(Arc::new(starter))));

    let first = tokio::spawn(
        Arc::clone(&service).transcribe(Bytes::from_static(FIXTURE), AudioFormat::OggOpus),
    );
    tokio::task::spawn_blocking(move || entered_seen.recv())
        .await
        .expect("join")
        .expect("the first engine call starts");
    let second = Arc::clone(&service)
        .transcribe(Bytes::from_static(FIXTURE), AudioFormat::OggOpus)
        .await;
    assert_eq!(second.expect_err("busy"), VoiceFailure::Busy);

    release.send(()).expect("release");
    let first = first.await.expect("join").expect("the first one finishes");
    assert_eq!(first.text, "held");
}

#[tokio::test]
async fn a_lost_worker_is_restarted_by_the_next_request() {
    let (service, starts) = service(Behaviour::Lose, false);
    for _ in 0..2 {
        let failure = service
            .clone()
            .transcribe(Bytes::from_static(FIXTURE), AudioFormat::OggOpus)
            .await
            .expect_err("the worker timed out");
        assert_eq!(failure, VoiceFailure::Unavailable);
    }
    assert_eq!(starts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_worker_that_reports_an_error_is_kept_and_answers_internal() {
    let (service, starts) = service(Behaviour::Refuse, false);
    for _ in 0..2 {
        let failure = service
            .clone()
            .transcribe(Bytes::from_static(FIXTURE), AudioFormat::OggOpus)
            .await
            .expect_err("the engine refused");
        assert_eq!(failure, VoiceFailure::Internal);
    }
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn three_failed_starts_in_a_row_stop_the_engine_until_restart() {
    let (service, starts) = service(Behaviour::Answer, true);
    for _ in 0..4 {
        let failure = service
            .clone()
            .transcribe(Bytes::from_static(FIXTURE), AudioFormat::OggOpus)
            .await
            .expect_err("the worker does not start");
        assert_eq!(failure, VoiceFailure::Unavailable);
    }
    assert_eq!(starts.load(Ordering::SeqCst), 3);
}

/// The warm-up releases its permit last, so the permit is back once it has finished.
async fn wait_for_warm_up(service: &VoiceService) {
    for _ in 0..500 {
        if service.busy.available_permits() == 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the warm-up did not finish");
}

#[tokio::test]
async fn a_warm_up_loads_the_engine_before_the_first_note() {
    let (service, starts) = service(Behaviour::Answer, false);
    Arc::clone(&service).warm_up();
    wait_for_warm_up(&service).await;
    service
        .clone()
        .transcribe(Bytes::from_static(FIXTURE), AudioFormat::OggOpus)
        .await
        .expect("the warmed engine transcribes");
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_warm_up_without_the_engine_does_nothing() {
    let service = Arc::new(VoiceService::new(None));
    Arc::clone(&service).warm_up();
    let failure = service
        .transcribe(Bytes::from_static(FIXTURE), AudioFormat::OggOpus)
        .await
        .expect_err("no engine");
    assert_eq!(failure, VoiceFailure::Unavailable);
}

#[test]
fn the_log_line_carries_numbers_and_never_text() {
    let levels = AudioLevels {
        seconds: 2.5,
        peak_dbfs: -6.0,
        rms_dbfs: -18.3,
        clipped_percent: 0.0,
        speech_seconds: 1.8,
        chunks: 2,
        noise_floor_dbfs: -60.0,
        threshold_dbfs: -48.0,
    };
    assert_eq!(
        log_line(4096, 1840, Some(&levels)),
        "voice: 4096 bytes, 1840 ms; 2.5s audio, pico -6.0 dBFS, rms -18.3 dBFS, recorte 0.0%, \
         voz 1.8s en 2 tramos, suelo -60.0 dBFS, umbral -48.0 dBFS"
    );
    assert_eq!(log_line(10, 3, None), "voice: 10 bytes, 3 ms");
}

#[tokio::test]
async fn a_failed_warm_up_counts_as_a_failed_start() {
    let (service, starts) = service(Behaviour::Answer, true);
    Arc::clone(&service).warm_up();
    wait_for_warm_up(&service).await;
    for _ in 0..3 {
        let failure = service
            .clone()
            .transcribe(Bytes::from_static(FIXTURE), AudioFormat::OggOpus)
            .await
            .expect_err("the worker does not start");
        assert_eq!(failure, VoiceFailure::Unavailable);
    }
    assert_eq!(starts.load(Ordering::SeqCst), 3);
}

/// Counts its own drops, so a test sees that `suspend` ends the worker.
struct Tracked(Arc<AtomicUsize>);

impl Engine for Tracked {
    fn transcribe(&mut self, _pcm: &[f32]) -> Result<String, AsrError> {
        Ok("tracked".to_owned())
    }
}

impl Drop for Tracked {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

struct TrackedStarter {
    starts: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

impl EngineStarter for TrackedStarter {
    fn start(&self) -> Result<Box<dyn Engine>, AsrError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Tracked(Arc::clone(&self.drops))))
    }
}

fn tracked_service() -> (Arc<VoiceService>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let starts = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let starter = TrackedStarter {
        starts: Arc::clone(&starts),
        drops: Arc::clone(&drops),
    };
    (
        Arc::new(VoiceService::new(Some(Arc::new(starter)))),
        starts,
        drops,
    )
}

#[tokio::test]
async fn a_suspend_releases_the_engine_and_nothing_reopens_it() {
    let (service, starts, drops) = tracked_service();
    Arc::clone(&service).warm_up();
    wait_for_warm_up(&service).await;
    service.suspend();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(service.state(), "suspended");

    Arc::clone(&service).warm_up();
    wait_for_warm_up(&service).await;
    let failure = service
        .clone()
        .transcribe(Bytes::from_static(FIXTURE), AudioFormat::OggOpus)
        .await
        .expect_err("suspended");
    assert_eq!(failure, VoiceFailure::Unavailable);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_resume_loads_the_engine_again() {
    let (service, starts, _drops) = tracked_service();
    service.suspend();
    Arc::clone(&service).resume();
    wait_for_warm_up(&service).await;
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(service.state(), "ready");
    service
        .clone()
        .transcribe(Bytes::from_static(FIXTURE), AudioFormat::OggOpus)
        .await
        .expect("the resumed engine transcribes");
}
