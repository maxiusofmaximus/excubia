//! The Graph client for drafts, and only drafts.
//!
//! The whole surface is `/me/messages` and `/me/messages/{id}/attachments`:
//! create, update, read metadata, attach a file. There is no send and no delete.
//! A draft is only addressable here when the [`Ledger`] says this account created
//! it. Reading received mail is GET-only and lives in [`crate::inbox`].
//!
//! Repeating `create_draft` with identical content within fifteen minutes
//! returns the same draft: the key is the SHA-256 of the canonical payload.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::draft::{validate_attachment, AttachmentMeta, DraftInput, DraftMeta, SmallAttachment};
use crate::error::MailError;
use crate::ledger::{digest_hex, Ledger};
use crate::redact::Redacted;
use crate::transport::{HttpResponse, HttpTransport};

pub const GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";

/// Graph accepts a simple attachment below 3 MiB; from there an upload session
/// is mandatory (MAIL1B).
pub const SMALL_ATTACHMENT_LIMIT: usize = 3 * 1024 * 1024;

const META_SELECT: &str = "id,subject,toRecipients,ccRecipients,isDraft,hasAttachments";

/// A client bound to one access token and one account. What it creates is
/// recorded in the ledger, so ownership survives a restart and a token refresh.
pub struct GraphClient<'a> {
    pub(crate) transport: &'a dyn HttpTransport,
    pub(crate) access_token: &'a str,
    pub(crate) ledger: &'a Ledger,
    pub(crate) account: &'a str,
}

impl<'a> GraphClient<'a> {
    pub fn new(
        transport: &'a dyn HttpTransport,
        access_token: &'a str,
        ledger: &'a Ledger,
        account: &'a str,
    ) -> Self {
        Self {
            transport,
            access_token,
            ledger,
            account,
        }
    }

    /// Creates the draft, or returns the one this content created within the window.
    pub async fn create_draft(&self, input: &DraftInput) -> Result<DraftMeta, MailError> {
        let payload = input.payload()?;
        let hash = payload_digest(&payload)?;
        if let Some(id) = self.ledger.draft_for_content(self.account, &hash)? {
            return self.get_draft_meta(&id).await;
        }
        let response = self
            .transport
            .post_json(
                &format!("{GRAPH_BASE}/me/messages"),
                self.access_token,
                &payload,
            )
            .await?;
        let meta = message(response)?.into_meta(Vec::new());
        self.ledger.remember_draft(self.account, &meta.id, &hash)?;
        self.ledger.remember_body(&meta.id, input.body.content())?;
        Ok(meta)
    }

    pub async fn update_draft(
        &self,
        draft_id: &str,
        input: &DraftInput,
    ) -> Result<DraftMeta, MailError> {
        self.owned(draft_id)?;
        let payload = input.payload()?;
        let response = self
            .transport
            .patch_json(
                &format!("{GRAPH_BASE}/me/messages/{}", encode_segment(draft_id)),
                self.access_token,
                &payload,
            )
            .await?;
        let meta = message(response)?.into_meta(self.ledger.attachment_names(draft_id)?);
        self.ledger.forget_content(draft_id)?;
        self.ledger.remember_body(draft_id, input.body.content())?;
        Ok(meta)
    }

    pub async fn get_draft_meta(&self, draft_id: &str) -> Result<DraftMeta, MailError> {
        self.owned(draft_id)?;
        let url = format!(
            "{GRAPH_BASE}/me/messages/{}?$select={META_SELECT}",
            encode_segment(draft_id)
        );
        let response = self.transport.get_json(&url, self.access_token).await?;
        let raw = message(response)?;
        let names = if raw.has_attachments {
            self.attachment_names(draft_id).await?
        } else {
            Vec::new()
        };
        Ok(raw.into_meta(names))
    }

    pub async fn add_small_attachment(
        &self,
        draft_id: &str,
        attachment: &SmallAttachment<'_>,
    ) -> Result<AttachmentMeta, MailError> {
        if attachment.bytes.len() >= SMALL_ATTACHMENT_LIMIT {
            return Err(MailError::UseUploadSession {
                size: attachment.bytes.len() as u64,
            });
        }
        validate_attachment(attachment)?;
        self.owned(draft_id)?;
        let payload = json!({
            "@odata.type": "#microsoft.graph.fileAttachment",
            "name": attachment.file_name,
            "contentType": attachment.content_type,
            "contentBytes": STANDARD.encode(attachment.bytes),
        });
        let response = self
            .transport
            .post_json(
                &format!(
                    "{GRAPH_BASE}/me/messages/{}/attachments",
                    encode_segment(draft_id)
                ),
                self.access_token,
                &payload,
            )
            .await?;
        check_status(&response)?;
        let meta = parse_attachment(&response.body)?;
        self.record_attachment(draft_id, &digest_hex(attachment.bytes), &meta)?;
        Ok(meta)
    }

    async fn attachment_names(&self, draft_id: &str) -> Result<Vec<String>, MailError> {
        let url = format!(
            "{GRAPH_BASE}/me/messages/{}/attachments?$select=name",
            encode_segment(draft_id)
        );
        let response = self.transport.get_json(&url, self.access_token).await?;
        check_status(&response)?;
        let raw: RawAttachments = serde_json::from_str(&response.body).map_err(|_| {
            MailError::BadResponse(Redacted::new("the attachment list is incomplete"))
        })?;
        Ok(raw.value.into_iter().map(|item| item.name).collect())
    }

    /// Refuses a draft this account did not create, or no longer remembers.
    pub(crate) fn owned(&self, draft_id: &str) -> Result<(), MailError> {
        if self.ledger.owns(self.account, draft_id)? {
            return Ok(());
        }
        Err(MailError::DraftNotOwned {
            id: Redacted::new(draft_id),
        })
    }

    /// The attachment already accepted for these bytes under this name, if any.
    pub(crate) fn attached(
        &self,
        draft_id: &str,
        sha256: &str,
        name: &str,
    ) -> Result<Option<AttachmentMeta>, MailError> {
        self.ledger.attachment(draft_id, sha256, name)
    }

    /// Records an attachment Graph accepted, under the digest of its bytes.
    pub(crate) fn record_attachment(
        &self,
        draft_id: &str,
        sha256: &str,
        meta: &AttachmentMeta,
    ) -> Result<(), MailError> {
        self.ledger
            .remember_attachment(self.account, draft_id, sha256, meta)
    }
}

fn payload_digest(payload: &Value) -> Result<String, MailError> {
    let bytes = serde_json::to_vec(payload)
        .map_err(|_| MailError::BadResponse(Redacted::new("the draft payload is not JSON")))?;
    Ok(digest_hex(&bytes))
}

/// Refuses anything but a 2xx answer. A 401 means the token must be refreshed.
pub(crate) fn check_status(response: &HttpResponse) -> Result<(), MailError> {
    if response.status == 401 {
        return Err(MailError::AuthRequired);
    }
    if response.is_success() {
        return Ok(());
    }
    Err(graph_error(response))
}

pub(crate) fn parse_attachment(body: &str) -> Result<AttachmentMeta, MailError> {
    let raw: RawAttachment = serde_json::from_str(body).map_err(|_| {
        MailError::BadResponse(Redacted::new("the attachment answer is incomplete"))
    })?;
    Ok(AttachmentMeta {
        id: raw.id,
        name: raw.name,
        size: raw.size,
    })
}

/// The sign-in name of the account, read once at login from `/me`.
pub async fn fetch_upn(
    transport: &dyn HttpTransport,
    access_token: &str,
) -> Result<String, MailError> {
    let url = format!("{GRAPH_BASE}/me?$select=userPrincipalName");
    let response = transport.get_json(&url, access_token).await?;
    check_status(&response)?;
    let profile: RawProfile = serde_json::from_str(&response.body).map_err(|_| {
        MailError::BadResponse(Redacted::new("Graph answered with an incomplete profile"))
    })?;
    Ok(profile.user_principal_name)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawProfile {
    user_principal_name: String,
}

fn message(response: HttpResponse) -> Result<RawMessage, MailError> {
    check_status(&response)?;
    serde_json::from_str(&response.body).map_err(|_| {
        MailError::BadResponse(Redacted::new("Graph answered with an incomplete message"))
    })
}

fn graph_error(response: &HttpResponse) -> MailError {
    let body: Value = serde_json::from_str(&response.body).unwrap_or(Value::Null);
    let code = body
        .pointer("/error/code")
        .and_then(Value::as_str)
        .unwrap_or("unknown_error");
    let text = body
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("Graph did not explain the refusal");
    MailError::Graph {
        status: response.status,
        code: code.to_owned(),
        message: Redacted::new(text),
    }
}

/// Percent-encode one path segment. Graph ids are opaque and may carry `!`,
/// `=` or characters a URL path cannot hold verbatim.
pub(crate) fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMessage {
    id: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    to_recipients: Vec<RawRecipient>,
    #[serde(default)]
    cc_recipients: Vec<RawRecipient>,
    #[serde(default)]
    is_draft: bool,
    #[serde(default)]
    has_attachments: bool,
}

impl RawMessage {
    fn into_meta(self, attachment_names: Vec<String>) -> DraftMeta {
        DraftMeta {
            id: self.id,
            subject: self.subject,
            to: addresses(self.to_recipients),
            cc: addresses(self.cc_recipients),
            is_draft: self.is_draft,
            attachment_names,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRecipient {
    email_address: RawAddress,
}

#[derive(Deserialize)]
struct RawAddress {
    address: String,
}

fn addresses(recipients: Vec<RawRecipient>) -> Vec<String> {
    recipients
        .into_iter()
        .map(|recipient| recipient.email_address.address)
        .collect()
}

#[derive(Deserialize)]
struct RawAttachments {
    #[serde(default)]
    value: Vec<RawAttachment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAttachment {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: u64,
}
