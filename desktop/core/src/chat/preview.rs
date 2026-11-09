//! Page previews of an xlsx, docx or pdf attachment: the host renders one page to a
//! PNG and this side checks it before the webview gets it. Refusals keep their code.

use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::Method;
use serde::Serialize;
use serde_json::Value;

use super::attachments::attachment_path;
use crate::client::HostClient;

/// A page larger than this is refused: the viewer is a screen, not a print.
pub const MAX_PREVIEW_BYTES: usize = 8 * 1024 * 1024;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const OTHER_TEXT: &str = "No se pudo mostrar la página. Reinténtalo.";
const UNAVAILABLE_TEXT: &str = "Este archivo no tiene vista previa.";
const OUT_OF_RANGE_TEXT: &str = "Esa página no existe en el documento.";
const TOOL_MISSING_TEXT: &str = "Falta una herramienta de documentos en este PC.";

/// Why a page is not shown. The webview words it by `code`; `message` is Spanish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum PreviewFailure {
    /// The file has no preview: csv, txt, md, json or pptx.
    Unavailable { message: String },
    /// The document has no page with that number.
    OutOfRange { message: String },
    /// A document tool is not installed on this PC; the message names which one.
    ToolMissing { message: String },
    /// Anything else: no connection, a failed render, a body that is not a PNG.
    Other { message: String },
}

impl PreviewFailure {
    pub fn other() -> Self {
        Self::Other {
            message: OTHER_TEXT.into(),
        }
    }
}

/// A page ready to draw: base64 PNG, the way the image attachments travel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewPage {
    pub mime: &'static str,
    pub data: String,
}

/// The host's refusal of a preview, by its `detail.code`. Anything it does not name is `Other`.
fn preview_failure(body: &[u8]) -> PreviewFailure {
    let Some(detail) = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("detail").cloned())
    else {
        return PreviewFailure::other();
    };
    match detail.get("code").and_then(Value::as_str) {
        Some("preview_unavailable") => PreviewFailure::Unavailable {
            message: UNAVAILABLE_TEXT.into(),
        },
        Some("page_out_of_range") => PreviewFailure::OutOfRange {
            message: OUT_OF_RANGE_TEXT.into(),
        },
        Some("tool_missing") => PreviewFailure::ToolMissing {
            message: detail
                .get("message")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .unwrap_or(TOOL_MISSING_TEXT)
                .to_string(),
        },
        _ => PreviewFailure::other(),
    }
}

/// Only a PNG within the limit goes to the webview; anything else is a failed render.
fn checked_png(bytes: &[u8]) -> Result<(), PreviewFailure> {
    if bytes.len() <= MAX_PREVIEW_BYTES && bytes.starts_with(PNG_SIGNATURE) {
        Ok(())
    } else {
        Err(PreviewFailure::other())
    }
}

impl HostClient {
    /// The PNG of one page (from 1) of a ready xlsx, docx or pdf attachment.
    pub async fn chat_attachment_preview(
        &self,
        conversation_id: &str,
        attachment_id: &str,
        page: u32,
    ) -> Result<PreviewPage, PreviewFailure> {
        let path =
            attachment_path(conversation_id, attachment_id).map_err(|_| PreviewFailure::other())?;
        let request = self
            .signed_call(
                Method::GET,
                &format!("{path}/preview"),
                &[("page", page.to_string())],
                None,
            )
            .map_err(|_| PreviewFailure::other())?;
        let response = request.send().await.map_err(|_| PreviewFailure::other())?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|_| PreviewFailure::other())?;
        if !status.is_success() {
            return Err(preview_failure(&body));
        }
        checked_png(&body)?;
        Ok(PreviewPage {
            mime: "image/png",
            data: STANDARD.encode(&body),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{checked_png, preview_failure, PreviewFailure, MAX_PREVIEW_BYTES};

    const PNG_HEAD: &[u8] = b"\x89PNG\r\n\x1a\n";

    #[test]
    fn host_refusals_keep_their_code_and_the_hosts_tool_text() {
        let body = br#"{"detail":{"code":"tool_missing","message":"Falta GridCraft en este PC."}}"#;
        assert_eq!(
            preview_failure(body),
            PreviewFailure::ToolMissing {
                message: "Falta GridCraft en este PC.".into()
            }
        );
        let unavailable = br#"{"detail":{"code":"preview_unavailable","message":"x"}}"#;
        assert!(matches!(
            preview_failure(unavailable),
            PreviewFailure::Unavailable { .. }
        ));
        let out = br#"{"detail":{"code":"page_out_of_range","message":"x"}}"#;
        assert!(matches!(
            preview_failure(out),
            PreviewFailure::OutOfRange { .. }
        ));
    }

    #[test]
    fn a_failed_render_or_an_unknown_body_is_other() {
        for body in [
            &br#"{"detail":{"code":"preview_failed","message":"fixed"}}"#[..],
            br#"{"detail":{"code":"not_ready","message":"x"}}"#,
            br#"{"detail":"internal error"}"#,
            b"<html>502 Bad Gateway</html>",
            b"",
        ] {
            assert!(
                matches!(preview_failure(body), PreviewFailure::Other { .. }),
                "{:?}",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn a_blank_tool_message_falls_back_to_the_fixed_text() {
        let body = br#"{"detail":{"code":"tool_missing","message":"  "}}"#;
        assert!(matches!(
            preview_failure(body),
            PreviewFailure::ToolMissing { message } if !message.trim().is_empty()
        ));
    }

    #[test]
    fn only_a_png_under_eight_mib_is_passed_on() {
        let mut png = PNG_HEAD.to_vec();
        png.extend_from_slice(b"rest");
        assert!(checked_png(&png).is_ok());
        assert!(
            checked_png(b"\xff\xd8\xff\xe0jpeg").is_err(),
            "a JPEG is not a page"
        );
        assert!(checked_png(b"").is_err());
        let mut big = PNG_HEAD.to_vec();
        big.resize(MAX_PREVIEW_BYTES + 1, 0);
        assert!(checked_png(&big).is_err());
        big.truncate(MAX_PREVIEW_BYTES);
        assert!(checked_png(&big).is_ok());
    }

    #[test]
    fn the_webview_reads_a_code_and_a_message() {
        let value = serde_json::to_value(PreviewFailure::OutOfRange {
            message: "Esa página no existe.".into(),
        })
        .expect("json");
        assert_eq!(value["code"], "out_of_range");
        assert_eq!(value["message"], "Esa página no existe.");
        let other = serde_json::to_value(PreviewFailure::other()).expect("json");
        assert_eq!(other["code"], "other");
    }
}
