mod common;

use common::spawn_server;
use excubia_desktop_core::identity::DeviceIdentity;
use excubia_desktop_core::{HostClient, VoiceFailure, VoiceTranscript};

const TRANSCRIBED: &str = r#"{"text":"hola Excubia","ms":1840}"#;

fn client(base_url: &str) -> HostClient {
    HostClient::new(base_url, DeviceIdentity::generate("dev-test", "test")).unwrap()
}

#[tokio::test]
async fn a_note_goes_as_a_signed_wav_and_the_text_comes_back() {
    let (base_url, captured) = spawn_server(vec![(200, TRANSCRIBED)]).await;
    let wav = vec![b'R', b'I', 0x00, 0xff, 0x7f, 0x80];

    let transcript = client(&base_url).voice_transcribe(&wav).await.unwrap();
    assert_eq!(
        transcript,
        VoiceTranscript {
            text: "hola Excubia".to_string(),
            ms: 1840,
        }
    );

    let captured = captured.lock().unwrap();
    assert_eq!(captured[0].method, "POST");
    assert_eq!(captured[0].target, "/v1/voice/transcribe");
    assert_eq!(captured[0].body, wav);
    let headers = captured[0].headers.to_ascii_lowercase();
    assert!(headers.contains("content-type: audio/wav\r\n"), "{headers}");
    assert!(headers.contains("x-excubia-signature:"), "{headers}");
}

#[tokio::test]
async fn each_refusal_has_its_kind() {
    let cases = [
        (429, VoiceFailure::Busy),
        (403, VoiceFailure::Forbidden),
        (413, VoiceFailure::TooLarge),
        (503, VoiceFailure::Unavailable),
        (400, VoiceFailure::Rejected),
    ];
    for (status, expected) in cases {
        let (base_url, _) = spawn_server(vec![(status, r#"{"detail":{"code":"x"}}"#)]).await;
        let failure = client(&base_url)
            .voice_transcribe(b"RIFF")
            .await
            .unwrap_err();
        assert_eq!(failure, expected, "status {status}");
    }
}

#[tokio::test]
async fn an_answer_that_is_not_a_transcript_is_a_protocol_failure() {
    let (base_url, _) = spawn_server(vec![(200, r#"{"ok":true}"#)]).await;
    let failure = client(&base_url)
        .voice_transcribe(b"RIFF")
        .await
        .unwrap_err();
    assert_eq!(failure, VoiceFailure::Protocol);
}

#[tokio::test]
async fn no_host_on_the_port_is_offline() {
    let failure = client("http://127.0.0.1:9")
        .voice_transcribe(b"RIFF")
        .await
        .unwrap_err();
    assert_eq!(failure, VoiceFailure::Offline);
}
