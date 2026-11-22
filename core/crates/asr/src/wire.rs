//! Line protocol with `worker/asr_worker.py`: a JSON header line, then raw little-endian f32 PCM.
//! Replies are one JSON line each; the worker's stdout carries nothing else.

use serde::Deserialize;

use crate::error::AsrError;

const NO_CUDA_CODE: &str = "no_cuda";

pub(crate) fn encode_request(id: u64, pcm: &[f32]) -> Vec<u8> {
    let header = format!("{{\"id\":{id},\"samples\":{}}}\n", pcm.len());
    let mut frame = Vec::with_capacity(header.len() + pcm.len() * 4);
    frame.extend_from_slice(header.as_bytes());
    for sample in pcm {
        frame.extend_from_slice(&sample.to_le_bytes());
    }
    frame
}

/// Untagged: the worker's three line shapes are told apart by their keys alone.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum Reply {
    Text { id: u64, text: String },
    Error { error: String },
    Ready { provider: String },
}

/// The text of a reply to request `id`. A reply for another request is a protocol fault.
pub(crate) fn text_of(reply: Reply, id: u64) -> Result<String, AsrError> {
    match reply {
        Reply::Text { id: answered, text } if answered == id => Ok(text),
        Reply::Error { error } => Err(engine_error(error)),
        Reply::Text { .. } | Reply::Ready { .. } => Err(AsrError::InvalidReply),
    }
}

pub(crate) fn engine_error(code: String) -> AsrError {
    if code == NO_CUDA_CODE {
        AsrError::NoCuda
    } else {
        AsrError::Engine(code)
    }
}

#[cfg(test)]
mod tests {
    use super::{encode_request, text_of, Reply};
    use crate::error::AsrError;

    #[test]
    fn request_is_a_header_line_then_little_endian_f32() {
        let frame = encode_request(7, &[1.0, -0.5]);
        let newline = frame.iter().position(|&byte| byte == b'\n').unwrap();
        assert_eq!(&frame[..newline], br#"{"id":7,"samples":2}"#);
        assert_eq!(&frame[newline + 1..], &[0, 0, 0x80, 0x3f, 0, 0, 0, 0xbf]);
    }

    #[test]
    fn the_three_reply_shapes_are_told_apart() {
        let text: Reply = serde_json::from_str(r#"{"id":3,"text":"hola","ms":12}"#).unwrap();
        let error: Reply = serde_json::from_str(r#"{"id":3,"error":"timeout"}"#).unwrap();
        let ready: Reply =
            serde_json::from_str(r#"{"ready":true,"provider":"CUDAExecutionProvider"}"#).unwrap();
        assert_eq!(text_of(text, 3), Ok("hola".to_owned()));
        assert_eq!(
            text_of(error, 3),
            Err(AsrError::Engine("timeout".to_owned()))
        );
        assert!(matches!(ready, Reply::Ready { .. }));
    }

    #[test]
    fn a_reply_for_another_request_is_refused() {
        let reply: Reply = serde_json::from_str(r#"{"id":4,"text":"x","ms":1}"#).unwrap();
        assert_eq!(text_of(reply, 3), Err(AsrError::InvalidReply));
    }

    #[test]
    fn no_cuda_code_maps_to_its_own_error() {
        let reply: Reply = serde_json::from_str(r#"{"error":"no_cuda"}"#).unwrap();
        assert_eq!(text_of(reply, 0), Err(AsrError::NoCuda));
    }
}
