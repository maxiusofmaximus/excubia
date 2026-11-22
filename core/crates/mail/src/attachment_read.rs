//! Reading the content of one attachment of a received message (TASK-ATT1). Two GETs
//! on the attachment: its metadata first, so a declared size over the limit is refused
//! before any content is requested; then its bytes. Only file attachments are read: an
//! attached message, an event or a link is refused with its own error. Nothing is
//! sent, moved or stored here; the bytes go to the caller and nowhere else.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Deserialize;

use crate::error::MailError;
use crate::graph::{check_status, encode_segment, GraphClient, GRAPH_BASE};
use crate::inbox::parse;
use crate::received::one_line;
use crate::redact::Redacted;
use crate::transport::HttpResponse;

/// The largest attachment a read takes: the size of a chat file.
pub const MAX_READ_BYTES: u64 = 10 * 1024 * 1024;

const META_SELECT: &str = "id,name,size";
const FILE_TYPE: &str = "#microsoft.graph.fileAttachment";
const MAX_NAME_CHARS: usize = 320;

/// The bytes of one file attachment, with the name its sender gave (untrusted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

impl GraphClient<'_> {
    /// Reads one file attachment of a message, of at most `max_bytes`. Read only: two GETs.
    pub async fn fetch_attachment(
        &self,
        message_id: &str,
        attachment_id: &str,
        max_bytes: u64,
    ) -> Result<AttachmentFile, MailError> {
        let base = format!(
            "{GRAPH_BASE}/me/messages/{}/attachments/{}",
            encode_segment(message_id),
            encode_segment(attachment_id)
        );
        let answer = self
            .get_attachment(&format!("{base}?$select={META_SELECT}"))
            .await?;
        let meta: RawMeta = parse(&answer, "the attachment is incomplete")?;
        // Graph marks the derived type with @odata.type; without it the type is unknown and is refused.
        if meta.odata_type != FILE_TYPE {
            return Err(MailError::AttachmentNotFile);
        }
        if meta.size > max_bytes {
            return Err(MailError::AttachmentTooLarge);
        }
        // No $select: `contentBytes` exists only on the derived fileAttachment type, so Graph
        // rejects selecting it on the base attachment resource.
        let answer = self.get_attachment(&base).await?;
        let content: RawContent = parse(&answer, "the attachment content is incomplete")?;
        let bytes = STANDARD.decode(content.content_bytes).map_err(|_| {
            MailError::BadResponse(Redacted::new("the attachment content is not base64"))
        })?;
        // The declared size can lie: the real one is checked too.
        if bytes.len() as u64 > max_bytes {
            return Err(MailError::AttachmentTooLarge);
        }
        Ok(AttachmentFile {
            name: one_line(&meta.name, MAX_NAME_CHARS),
            bytes,
        })
    }

    /// One GET on an attachment: a 404 means the message has no such attachment.
    async fn get_attachment(&self, url: &str) -> Result<HttpResponse, MailError> {
        let response = self.transport.get_json(url, self.access_token).await?;
        if response.status == 404 {
            return Err(MailError::AttachmentMissing);
        }
        check_status(&response)?;
        Ok(response)
    }
}

#[derive(Deserialize)]
struct RawMeta {
    #[serde(rename = "@odata.type", default)]
    odata_type: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawContent {
    #[serde(default)]
    content_bytes: String,
}
