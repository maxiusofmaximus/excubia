//! The two Graph calls of a send (MAIL2A), and the only ones that read a body
//! or reach `/send`. The read runs with the login token; the POST runs with
//! the send token and touches that one endpoint and nothing else.

use serde::Deserialize;

use crate::draft::ASCII_TEXT_PADDING;
use crate::error::MailError;
use crate::graph::{check_status, encode_segment, GraphClient, GRAPH_BASE};
use crate::ledger::digest_hex;
use crate::markup::TEXT_CONTENT_TYPE;
use crate::redact::Redacted;
use crate::send::{preview_of, DraftReview, SendAttachment, SendView};

const SEND_SELECT: &str = "id,subject,from,toRecipients,ccRecipients,isDraft,body";

impl GraphClient<'_> {
    /// The draft as the owner is asked to approve it: the fields the digest
    /// covers, the body as a digest and a short preview, and the attachments
    /// with the SHA-256 Excubia recorded for each.
    pub async fn read_for_send(&self, draft_id: &str) -> Result<SendView, MailError> {
        let review = self.read_for_review(draft_id).await?;
        let body_preview = preview_of(&review.text());
        Ok(SendView {
            account: review.account,
            draft_id: review.draft_id,
            from: review.from,
            to: review.to,
            cc: review.cc,
            subject: review.subject,
            body_sha256: digest_hex(review.body.as_bytes()),
            body_preview,
            attachments: review.attachments,
        })
    }

    /// The draft in full, for the operator to read before approving. A read
    /// only: the draft is not changed.
    pub async fn read_for_review(&self, draft_id: &str) -> Result<DraftReview, MailError> {
        self.owned(draft_id)?;
        let url = format!(
            "{GRAPH_BASE}/me/messages/{}?$select={SEND_SELECT}",
            encode_segment(draft_id)
        );
        let response = self.transport.get_json(&url, self.access_token).await?;
        check_status(&response)?;
        let raw: RawSendMessage = parse(&response.body, "the draft is incomplete")?;
        if !raw.is_draft {
            return Err(MailError::InvalidInput(Redacted::new(
                "the message is not a draft and cannot be sent",
            )));
        }
        let attachments = self.send_attachments(draft_id).await?;
        let content_type = raw.body.content_type;
        Ok(DraftReview {
            account: self.account.to_owned(),
            draft_id: draft_id.to_owned(),
            from: raw
                .from
                .map(|sender| sender.email_address.address)
                .unwrap_or_default(),
            to: addresses(raw.to_recipients),
            cc: addresses(raw.cc_recipients),
            subject: raw.subject,
            body: self.owner_body(draft_id, raw.body.content, &content_type)?,
            content_type,
            attachments,
        })
    }

    /// The body as the owner wrote it: the hard space this host adds to an
    /// all-ASCII text body is dropped only when what precedes it is exactly the
    /// body the host wrote, so a hard space the owner wrote is kept.
    fn owner_body(
        &self,
        draft_id: &str,
        body: String,
        content_type: &str,
    ) -> Result<String, MailError> {
        if !content_type.eq_ignore_ascii_case(TEXT_CONTENT_TYPE)
            || !body.ends_with(ASCII_TEXT_PADDING)
        {
            return Ok(body);
        }
        let written = self.ledger.body_sha256(draft_id)?;
        let end = body.len() - ASCII_TEXT_PADDING.len_utf8();
        match written {
            Some(sha256) if sha256 == digest_hex(&body.as_bytes()[..end]) => {
                Ok(body[..end].to_owned())
            }
            _ => Ok(body),
        }
    }

    async fn send_attachments(&self, draft_id: &str) -> Result<Vec<SendAttachment>, MailError> {
        let url = format!(
            "{GRAPH_BASE}/me/messages/{}/attachments?$select=name,size",
            encode_segment(draft_id)
        );
        let response = self.transport.get_json(&url, self.access_token).await?;
        check_status(&response)?;
        let raw: RawAttachmentList = parse(&response.body, "the attachment list is incomplete")?;
        let digests = self.ledger.attachment_digests(draft_id)?;
        Ok(raw
            .value
            .into_iter()
            .map(|item| {
                let sha256 = digests
                    .iter()
                    .find(|(name, _)| *name == item.name)
                    .map(|(_, digest)| digest.clone());
                SendAttachment {
                    name: item.name,
                    size: item.size,
                    sha256,
                }
            })
            .collect())
    }

    /// `POST /me/messages/{id}/send`: the only call of this client that sends.
    /// Refuses a draft this account did not create before it touches Graph.
    pub async fn send_draft(&self, draft_id: &str) -> Result<(), MailError> {
        self.owned(draft_id)?;
        let url = format!("{GRAPH_BASE}/me/messages/{}/send", encode_segment(draft_id));
        let response = self.transport.post_action(&url, self.access_token).await?;
        check_status(&response)
    }
}

fn parse<T: for<'de> Deserialize<'de>>(body: &str, reason: &str) -> Result<T, MailError> {
    serde_json::from_str(body)
        .map_err(|_| MailError::BadResponse(Redacted::new(format!("Graph answered: {reason}"))))
}

fn addresses(recipients: Vec<RawRecipient>) -> Vec<String> {
    recipients
        .into_iter()
        .map(|recipient| recipient.email_address.address)
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSendMessage {
    #[serde(default)]
    subject: String,
    from: Option<RawRecipient>,
    #[serde(default)]
    to_recipients: Vec<RawRecipient>,
    #[serde(default)]
    cc_recipients: Vec<RawRecipient>,
    is_draft: bool,
    body: RawBody,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBody {
    content: String,
    #[serde(default)]
    content_type: String,
}

#[derive(Deserialize)]
struct RawAttachmentList {
    value: Vec<RawAttachmentItem>,
}

#[derive(Deserialize)]
struct RawAttachmentItem {
    name: String,
    size: u64,
}
