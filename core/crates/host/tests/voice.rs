//! The voice route at HTTP level: a signed voice note in, its text out, and the refusals of the
//! contract. The engine is a fake; the audio is a real Ogg Opus fixture, decoded for real.

mod harness;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use harness::TestHost;
use hn_core_api::voice::{Engine, EngineStarter, MAX_VOICE_BODY_BYTES};
use hn_core_asr::AsrError;
use hn_core_auth::{canonical_string, Permission};
use hn_core_test_support::DemoIdentity;
use serde_json::Value;

const FIXTURE: &[u8] = include_bytes!("../../asr/tests/fixtures/tones_2s5.opus");
const PATH: &str = "/v1/voice/transcribe";
const OGG: &str = "audio/ogg";
static NONCE: AtomicU64 = AtomicU64::new(0);

/// Answers with the sample count it was given, so the decoded audio is visible in the text.
struct Echo(Arc<AtomicUsize>);

impl Engine for Echo {
    fn transcribe(&mut self, pcm: &[f32]) -> Result<String, AsrError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(format!("falso {} muestras", pcm.len()))
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
        Ok("liberado".to_owned())
    }
}

/// Hands out the one engine the test built.
struct Once(Mutex<Option<Box<dyn Engine>>>);

impl EngineStarter for Once {
    fn start(&self) -> Result<Box<dyn Engine>, AsrError> {
        let mut slot = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        slot.take().ok_or(AsrError::WorkerStart)
    }
}

fn once(engine: Box<dyn Engine>) -> Option<Arc<dyn EngineStarter>> {
    Some(Arc::new(Once(Mutex::new(Some(engine)))))
}

/// One signed request with the given content type. The shared test client always sends JSON,
/// so the voice tests sign their own requests.
async fn voice(
    base: &str,
    identity: &DemoIdentity,
    content_type: Option<&str>,
    body: Vec<u8>,
) -> (u16, reqwest::header::HeaderMap, Value) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs() as i64;
    let nonce = STANDARD.encode(format!("voz-{}", NONCE.fetch_add(1, Ordering::SeqCst)));
    let canonical = canonical_string("POST", PATH, &body, timestamp, &nonce).expect("canonical");
    let mut request = reqwest::Client::new()
        .post(format!("{base}{PATH}"))
        .header("X-HN-Device", identity.device_id())
        .header("X-HN-Timestamp", timestamp.to_string())
        .header("X-HN-Nonce", nonce)
        .header("X-HN-Signature", identity.sign(&canonical))
        .body(body);
    if let Some(content_type) = content_type {
        request = request.header("Content-Type", content_type);
    }
    let response = request.send().await.expect("send");
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let text = response.text().await.expect("body");
    (
        status,
        headers,
        serde_json::from_str(&text).unwrap_or(Value::Null),
    )
}

fn files_under(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("data dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            files_under(&path, found);
        } else {
            found.push(path);
        }
    }
}

#[tokio::test]
async fn a_signed_voice_note_comes_back_as_text_and_is_written_nowhere() {
    let calls = Arc::new(AtomicUsize::new(0));
    let host =
        TestHost::start_with_voice("voice-ok", once(Box::new(Echo(Arc::clone(&calls))))).await;
    let client = host.paired_with("voz", &[Permission::Respond]).await;

    let (status, _, body) = voice(
        &host.base_url(),
        client.identity(),
        Some(OGG),
        FIXTURE.to_vec(),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let text = body["text"].as_str().expect("text");
    let samples: usize = text
        .strip_prefix("falso ")
        .and_then(|rest| rest.strip_suffix(" muestras"))
        .and_then(|count| count.parse().ok())
        .expect("the fake's answer");
    assert!(samples >= 30_000, "2.5 s of audio, got {samples} samples");
    assert!(body["ms"].is_u64());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let mut files = Vec::new();
    files_under(&host.data_dir, &mut files);
    let needle = &FIXTURE[64..128];
    for file in files {
        let bytes = std::fs::read(&file).expect("file");
        assert!(
            !bytes.windows(needle.len()).any(|window| window == needle),
            "the audio was written to {}",
            file.display()
        );
    }
}

#[tokio::test]
async fn an_unsigned_voice_note_is_unauthorized() {
    let host = TestHost::start_with_voice(
        "voice-401",
        once(Box::new(Echo(Arc::new(AtomicUsize::new(0))))),
    )
    .await;
    let response = reqwest::Client::new()
        .post(format!("{}{PATH}", host.base_url()))
        .header("Content-Type", OGG)
        .body(FIXTURE.to_vec())
        .send()
        .await
        .expect("send");
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn a_device_without_respond_is_forbidden_before_the_engine_runs() {
    let calls = Arc::new(AtomicUsize::new(0));
    let host =
        TestHost::start_with_voice("voice-403", once(Box::new(Echo(Arc::clone(&calls))))).await;
    let reader = host.paired_with("lector", &[]).await;

    let (status, _, _) = voice(
        &host.base_url(),
        reader.identity(),
        Some(OGG),
        FIXTURE.to_vec(),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_body_over_the_cap_is_413_and_the_engine_never_sees_it() {
    let calls = Arc::new(AtomicUsize::new(0));
    let host =
        TestHost::start_with_voice("voice-413", once(Box::new(Echo(Arc::clone(&calls))))).await;
    let client = host.paired_with("voz", &[Permission::Respond]).await;

    let too_big = vec![0u8; MAX_VOICE_BODY_BYTES + 1];
    let (status, _, body) = voice(&host.base_url(), client.identity(), Some(OGG), too_big).await;
    assert_eq!(status, 413);
    assert_eq!(body["detail"], "body too large");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_body_at_the_cap_reaches_the_decoder() {
    let host = TestHost::start_with_voice(
        "voice-cap",
        once(Box::new(Echo(Arc::new(AtomicUsize::new(0))))),
    )
    .await;
    let client = host.paired_with("voz", &[Permission::Respond]).await;

    let at_cap = vec![0u8; MAX_VOICE_BODY_BYTES];
    let (status, _, body) = voice(&host.base_url(), client.identity(), Some(OGG), at_cap).await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["detail"]["code"], "invalid_audio");
}

#[tokio::test]
async fn another_content_type_is_refused_as_an_unsupported_type() {
    let host = TestHost::start_with_voice(
        "voice-type",
        once(Box::new(Echo(Arc::new(AtomicUsize::new(0))))),
    )
    .await;
    let client = host.paired_with("voz", &[Permission::Respond]).await;

    let (status, _, body) = voice(
        &host.base_url(),
        client.identity(),
        Some("application/json"),
        FIXTURE.to_vec(),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["detail"]["code"], "unsupported_type");
}

#[tokio::test]
async fn bytes_that_are_not_ogg_opus_are_invalid_audio() {
    let host = TestHost::start_with_voice(
        "voice-garbage",
        once(Box::new(Echo(Arc::new(AtomicUsize::new(0))))),
    )
    .await;
    let client = host.paired_with("voz", &[Permission::Respond]).await;

    let (status, _, body) = voice(
        &host.base_url(),
        client.identity(),
        Some(OGG),
        b"no soy audio".to_vec(),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["detail"]["code"], "invalid_audio");
}

/// A canonical 44-byte WAV: 16 kHz, mono, 16-bit PCM.
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

#[tokio::test]
async fn a_wav_note_is_transcribed_under_either_wav_type() {
    let calls = Arc::new(AtomicUsize::new(0));
    let host =
        TestHost::start_with_voice("voice-wav", once(Box::new(Echo(Arc::clone(&calls))))).await;
    let client = host.paired_with("voz", &[Permission::Respond]).await;

    for content_type in ["audio/wav", "audio/x-wav; charset=binary"] {
        let note = wav(&[16_384; 1_600]);
        let (status, _, body) = voice(
            &host.base_url(),
            client.identity(),
            Some(content_type),
            note,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["text"], "falso 1600 muestras");
    }
}

#[tokio::test]
async fn a_wav_outside_16k_mono_pcm16_is_unsupported_audio() {
    let calls = Arc::new(AtomicUsize::new(0));
    let host =
        TestHost::start_with_voice("voice-wav-no", once(Box::new(Echo(Arc::clone(&calls))))).await;
    let client = host.paired_with("voz", &[Permission::Respond]).await;

    // Offsets in the canonical header: channels at 22, bits at 34, format tag at 20, rate at 24.
    let mut stereo = wav(&[1, 2, 3, 4]);
    stereo[22] = 2;
    let mut eight_bit = wav(&[1, 2, 3, 4]);
    eight_bit[34] = 8;
    let mut float = wav(&[1, 2, 3, 4]);
    float[20] = 3;
    let mut other_rate = wav(&[1, 2, 3, 4]);
    other_rate[24..28].copy_from_slice(&44_100u32.to_le_bytes());
    let truncated = wav(&[1, 2, 3, 4])[..30].to_vec();
    for note in [stereo, eight_bit, float, other_rate, truncated, wav(&[])] {
        let (status, _, body) =
            voice(&host.base_url(), client.identity(), Some("audio/wav"), note).await;
        assert_eq!(status, 400, "{body}");
        assert_eq!(body["detail"]["code"], "unsupported_audio");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_host_without_speech_answers_voice_unavailable() {
    let host = TestHost::start_with_voice("voice-none", None).await;
    let client = host.paired_with("voz", &[Permission::Respond]).await;

    let (status, _, body) = voice(
        &host.base_url(),
        client.identity(),
        Some(OGG),
        FIXTURE.to_vec(),
    )
    .await;
    assert_eq!(status, 503);
    assert_eq!(body["detail"]["code"], "voice_unavailable");
}

#[tokio::test]
async fn a_second_voice_note_while_one_is_transcribed_is_busy() {
    let (entered, entered_seen) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let gate: Box<dyn Engine> = Box::new(Gate {
        entered,
        release: released,
    });
    let host = TestHost::start_with_voice("voice-busy", once(gate)).await;
    let client = host.paired_with("voz", &[Permission::Respond]).await;
    let base = host.base_url();
    let identity = client.identity();

    let second = async {
        tokio::task::spawn_blocking(move || entered_seen.recv())
            .await
            .expect("join")
            .expect("the first engine call starts");
        let answer = voice(&base, identity, Some(OGG), FIXTURE.to_vec()).await;
        release.send(()).expect("release");
        answer
    };
    let (first, (status, headers, body)) =
        tokio::join!(voice(&base, identity, Some(OGG), FIXTURE.to_vec()), second);

    assert_eq!(status, 429, "{body}");
    assert_eq!(headers["retry-after"], "2");
    assert_eq!(body["detail"]["code"], "voice_busy");
    assert_eq!(first.0, 200);
    assert_eq!(first.2["text"], "liberado");
}
