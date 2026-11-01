//! Voice notes: one signed WAV goes to the host and its transcript comes back.
//! Neither the audio nor the text is kept here; a refusal is a typed kind the
//! webview words in Spanish.

use std::time::Duration;

use reqwest::{Method, Response, StatusCode};
use serde::{Deserialize, Serialize};

use crate::client::GatewayClient;

/// The host may load the engine on the first note (up to 40 s) before it transcribes.
const VOICE_TIMEOUT: Duration = Duration::from_secs(90);

pub type VoiceResult<T> = Result<T, VoiceFailure>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VoiceFailure {
    /// No connection to the host, or no answer in time.
    Offline,
    /// Another note is being transcribed (429 `voice_busy`).
    Busy,
    /// This device has no RESPOND permission (403).
    Forbidden,
    /// The note is over the host's cap (413).
    TooLarge,
    /// The engine is not running on the host (503).
    Unavailable,
    /// Any other refusal of the host.
    Rejected,
    /// An answer that is not a transcript.
    Protocol,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceTranscript {
    pub text: String,
    pub ms: u64,
}

impl GatewayClient {
    /// Sends one WAV note (`audio/wav`) and returns its transcript. It is signed
    /// like the other chat writes, so it needs RESPOND.
    pub async fn voice_transcribe(&self, wav: &[u8]) -> VoiceResult<VoiceTranscript> {
        let request = self
            .signed_typed(
                Method::POST,
                "/v1/voice/transcribe",
                &[],
                Some((wav, "audio/wav")),
            )
            .map_err(|_| VoiceFailure::Protocol)?
            .timeout(VOICE_TIMEOUT);
        let response = request.send().await.map_err(|_| VoiceFailure::Offline)?;
        decode(response).await
    }
}

async fn decode(response: Response) -> VoiceResult<VoiceTranscript> {
    let status = response.status();
    let bytes = response.bytes().await.map_err(|_| VoiceFailure::Offline)?;
    if status.is_success() {
        return serde_json::from_slice(&bytes).map_err(|_| VoiceFailure::Protocol);
    }
    Err(refusal(status))
}

fn refusal(status: StatusCode) -> VoiceFailure {
    match status {
        StatusCode::TOO_MANY_REQUESTS => VoiceFailure::Busy,
        StatusCode::FORBIDDEN => VoiceFailure::Forbidden,
        StatusCode::PAYLOAD_TOO_LARGE => VoiceFailure::TooLarge,
        StatusCode::SERVICE_UNAVAILABLE => VoiceFailure::Unavailable,
        _ => VoiceFailure::Rejected,
    }
}
