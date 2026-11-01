//! Reading received mail (MAIL-LECTURA): list a folder and read one message. Both
//! are GET requests and nothing else; no message is marked, moved or deleted, and
//! an attachment's content is never asked for, only its name, type and size.

use serde::{Deserialize, Serialize};

use crate::error::MailError;
use crate::graph::{check_status, encode_segment, GraphClient, GRAPH_BASE};
use crate::received::{
    auth_summary, check_message, clean_body, cut, header_values, kept_headers, links, one_line,
    AttachmentInfo, Checks, KeptHeaders, Link, MAX_BODY_CHARS, MAX_LINKS,
};
use crate::redact::Redacted;
use crate::transport::HttpResponse;

const LIST_SELECT: &str = "id,subject,from,receivedDateTime,isRead,hasAttachments,bodyPreview";
const MESSAGE_SELECT: &str = "id,subject,from,sender,replyTo,toRecipients,ccRecipients,receivedDateTime,internetMessageId,hasAttachments,body,internetMessageHeaders";
const ATTACHMENT_SELECT: &str = "name,contentType,size,isInline";
const MAX_NAME_CHARS: usize = 320;
const MAX_PREVIEW_CHARS: usize = 200;
const MAX_SUBJECT_CHARS: usize = 998;

/// One line of a folder listing. The preview is the start of the body, never all of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InboxItem {
    pub id: String,
    pub subject: String,
    pub from_name: String,
    pub from_address: String,
    pub received_at: String,
    pub is_read: bool,
    pub has_attachments: bool,
    pub preview: String,
}

/// One received message with its checks, as `mail.message_get` returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReceivedMessage {
    pub id: String,
    pub subject: String,
    pub from_name: String,
    pub from_address: String,
    pub sender_address: String,
    pub reply_to: Vec<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub received_at: String,
    pub internet_message_id: String,
    pub has_attachments: bool,
    pub body_text: String,
    pub truncated: bool,
    pub attachments: Vec<AttachmentInfo>,
    pub links: Vec<Link>,
    pub headers: KeptHeaders,
    pub checks: Checks,
}

/// The request for a folder listing: newest first, with the optional filters.
/// `from` is a whole address when it has an `@`, otherwise a domain.
pub fn inbox_url(folder: &str, limit: u32, unread_only: bool, from: Option<&str>) -> String {
    let mut filters = Vec::new();
    if unread_only {
        filters.push("isRead eq false".to_owned());
    }
    if let Some(sender) = from {
        filters.push(sender_filter(sender));
    }
    let mut url = format!(
        "{GRAPH_BASE}/me/mailFolders/{folder}/messages?$top={limit}&$orderby={}&$select={LIST_SELECT}",
        encode_segment("receivedDateTime desc")
    );
    if !filters.is_empty() {
        url.push_str(&format!(
            "&$filter={}",
            encode_segment(&filters.join(" and "))
        ));
    }
    url
}

/// The sender filter. The value sits in single quotes, so each `'` is doubled.
fn sender_filter(sender: &str) -> String {
    let quoted = sender.replace('\'', "''");
    if sender.contains('@') {
        format!("from/emailAddress/address eq '{quoted}'")
    } else {
        format!("endswith(from/emailAddress/address,'@{quoted}')")
    }
}

impl GraphClient<'_> {
    /// Lists the newest messages of a folder. Read only: one GET.
    pub async fn list_inbox(
        &self,
        folder: &str,
        limit: u32,
        unread_only: bool,
        from: Option<&str>,
    ) -> Result<Vec<InboxItem>, MailError> {
        let url = inbox_url(folder, limit, unread_only, from);
        let response = self.transport.get_json(&url, self.access_token).await?;
        check_status(&response)?;
        let raw: RawList = parse(&response, "the inbox list is incomplete")?;
        Ok(raw.value.into_iter().map(RawListItem::into_item).collect())
    }

    /// Reads one received message: its body as text, its headers and its checks.
    /// Two GETs: the message and the metadata of its attachments.
    pub async fn read_message(&self, message_id: &str) -> Result<ReceivedMessage, MailError> {
        let url = format!(
            "{GRAPH_BASE}/me/messages/{}?$select={MESSAGE_SELECT}",
            encode_segment(message_id)
        );
        let response = self.transport.get_json(&url, self.access_token).await?;
        if response.status == 404 {
            return Err(MailError::MessageNotFound);
        }
        check_status(&response)?;
        let raw: RawMessage = parse(&response, "the message is incomplete")?;
        let attachments = self.attachments_of(message_id).await?;

        let pairs: Vec<(String, String)> = raw
            .internet_message_headers
            .into_iter()
            .map(|header| (header.name, header.value))
            .collect();
        let auth = auth_summary(
            &header_values(&pairs, "Authentication-Results"),
            header_values(&pairs, "Received-SPF").first().copied(),
        );
        let headers = kept_headers(&pairs);
        let text = clean_body(&raw.body.content, &raw.body.content_type);
        let mut found = if raw.body.content_type.eq_ignore_ascii_case("html") {
            links(&raw.body.content)
        } else {
            Vec::new()
        };
        let from = raw
            .from
            .map(|sender| sender.email_address)
            .unwrap_or_default();
        let sender = raw
            .sender
            .map(|sender| sender.email_address)
            .unwrap_or_default();
        let reply_to = addresses(raw.reply_to);
        let checks = check_message(
            &from.address,
            &reply_to,
            &pairs,
            auth,
            &text,
            &attachments,
            &found,
        );
        found.truncate(MAX_LINKS);
        let (body_text, truncated) = cut(&text, MAX_BODY_CHARS);
        Ok(ReceivedMessage {
            id: raw.id,
            subject: one_line(&raw.subject, MAX_SUBJECT_CHARS),
            from_name: one_line(&from.name, MAX_NAME_CHARS),
            from_address: from.address,
            sender_address: sender.address,
            reply_to,
            to: addresses(raw.to_recipients),
            cc: addresses(raw.cc_recipients),
            received_at: raw.received_date_time,
            internet_message_id: raw.internet_message_id,
            has_attachments: raw.has_attachments,
            body_text,
            truncated,
            attachments,
            links: found,
            headers,
            checks,
        })
    }

    /// The metadata of the attachments of one message. Their content is never requested.
    async fn attachments_of(&self, message_id: &str) -> Result<Vec<AttachmentInfo>, MailError> {
        let url = format!(
            "{GRAPH_BASE}/me/messages/{}/attachments?$select={ATTACHMENT_SELECT}",
            encode_segment(message_id)
        );
        let response = self.transport.get_json(&url, self.access_token).await?;
        check_status(&response)?;
        let raw: RawAttachments = parse(&response, "the attachment list is incomplete")?;
        Ok(raw
            .value
            .into_iter()
            .map(|item| AttachmentInfo {
                name: one_line(&item.name, MAX_NAME_CHARS),
                content_type: one_line(&item.content_type, MAX_NAME_CHARS),
                size: item.size,
                is_inline: item.is_inline,
            })
            .collect())
    }
}

pub(crate) fn parse<T: for<'de> Deserialize<'de>>(
    response: &HttpResponse,
    incomplete: &'static str,
) -> Result<T, MailError> {
    serde_json::from_str(&response.body)
        .map_err(|_| MailError::BadResponse(Redacted::new(incomplete)))
}

fn addresses(recipients: Vec<RawRecipient>) -> Vec<String> {
    recipients
        .into_iter()
        .map(|recipient| recipient.email_address.address)
        .collect()
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawAddress {
    #[serde(default)]
    name: String,
    #[serde(default)]
    address: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSender {
    #[serde(default)]
    email_address: RawAddress,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRecipient {
    email_address: RawAddress,
}

#[derive(Deserialize)]
struct RawList {
    #[serde(default)]
    value: Vec<RawListItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawListItem {
    id: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    from: Option<RawSender>,
    #[serde(default)]
    received_date_time: String,
    #[serde(default)]
    is_read: bool,
    #[serde(default)]
    has_attachments: bool,
    #[serde(default)]
    body_preview: String,
}

impl RawListItem {
    fn into_item(self) -> InboxItem {
        let from = self
            .from
            .map(|sender| sender.email_address)
            .unwrap_or_default();
        InboxItem {
            id: self.id,
            subject: one_line(&self.subject, MAX_SUBJECT_CHARS),
            from_name: one_line(&from.name, MAX_NAME_CHARS),
            from_address: from.address,
            received_at: self.received_date_time,
            is_read: self.is_read,
            has_attachments: self.has_attachments,
            preview: one_line(&self.body_preview, MAX_PREVIEW_CHARS),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMessage {
    id: String,
    #[serde(default)]
    subject: String,
    #[serde(default)]
    from: Option<RawSender>,
    #[serde(default)]
    sender: Option<RawSender>,
    #[serde(default)]
    reply_to: Vec<RawRecipient>,
    #[serde(default)]
    to_recipients: Vec<RawRecipient>,
    #[serde(default)]
    cc_recipients: Vec<RawRecipient>,
    #[serde(default)]
    received_date_time: String,
    #[serde(default)]
    internet_message_id: String,
    #[serde(default)]
    has_attachments: bool,
    #[serde(default)]
    body: RawBody,
    #[serde(default)]
    internet_message_headers: Vec<RawHeader>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawBody {
    #[serde(default)]
    content_type: String,
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct RawHeader {
    #[serde(default)]
    name: String,
    #[serde(default)]
    value: String,
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
    name: String,
    #[serde(default)]
    content_type: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    is_inline: bool,
}
