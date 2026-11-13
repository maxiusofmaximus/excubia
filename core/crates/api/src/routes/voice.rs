//! `POST /v1/voice/transcribe` (signed, `RESPOND`): a voice note in, its text out. Neither the
//! audio nor the text is stored or logged; the log keeps only the size and the duration.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use excubia_asr::MAX_AUDIO_SECONDS;
use serde_json::json;

use crate::error::ApiError;
use crate::voice::{AudioFormat, VoiceFailure};
use crate::ApiContext;

/// How long a client waits before it sends a note the engine refused as busy.
const VOICE_BUSY_RETRY_AFTER_SECONDS: u64 = 2;

pub async fn transcribe(
    State(ctx): State<ApiContext>,
    headers: HeaderMap,
    audio: Bytes,
) -> Response {
    let Some(format) = audio_format(&headers) else {
        return ApiError::action(
            "unsupported_type",
            "the voice note must be audio/ogg or audio/wav",
        )
        .into_response();
    };
    match ctx.voice.transcribe(audio, format).await {
        Ok(transcript) => {
            Json(json!({"text": transcript.text, "ms": transcript.ms})).into_response()
        }
        Err(failure) => ApiError::from(failure).into_response(),
    }
}

/// The format the `Content-Type` names; parameters after `;` do not count.
fn audio_format(headers: &HeaderMap) -> Option<AudioFormat> {
    let essence = headers
        .get(header::CONTENT_TYPE)?
        .to_str()
        .ok()?
        .split(';')
        .next()?
        .trim();
    match essence {
        "audio/ogg" => Some(AudioFormat::OggOpus),
        "audio/wav" | "audio/x-wav" => Some(AudioFormat::Wav),
        _ => None,
    }
}

impl From<VoiceFailure> for ApiError {
    fn from(failure: VoiceFailure) -> Self {
        match failure {
            VoiceFailure::InvalidAudio => {
                ApiError::action("invalid_audio", "the audio is not an Ogg Opus stream")
            }
            VoiceFailure::UnsupportedAudio => ApiError::action(
                "unsupported_audio",
                "the WAV must be 16-bit PCM, mono, 16000 Hz",
            ),
            VoiceFailure::AudioTooLong => ApiError::coded(
                StatusCode::PAYLOAD_TOO_LARGE,
                "audio_too_long",
                format!("the voice note is longer than {MAX_AUDIO_SECONDS} seconds"),
            ),
            VoiceFailure::Busy => ApiError {
                retry_after: Some(VOICE_BUSY_RETRY_AFTER_SECONDS),
                ..ApiError::coded(
                    StatusCode::TOO_MANY_REQUESTS,
                    "voice_busy",
                    "another voice note is being transcribed",
                )
            },
            VoiceFailure::Unavailable => ApiError::coded(
                StatusCode::SERVICE_UNAVAILABLE,
                "voice_unavailable",
                "speech to text is not available on this host",
            ),
            VoiceFailure::Internal => ApiError::internal("speech engine failed"),
        }
    }
}
