//! Sending a draft the owner approved (MAIL2A): what the approval shows, the
//! digest it binds, the states a send passes through and the hourly budget.
//!
//! Nothing here talks to Graph. The host drives the order (approve, window,
//! send), `graph_send.rs` makes the two calls, and this module decides what
//! counts as the same send.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::error::MailError;
use crate::ledger::digest_hex;
use crate::markup::text_of;

/// How much of the body the approval shows the owner.
pub const PREVIEW_CHARS: usize = 300;

/// The window the hourly budget counts over.
pub const RATE_WINDOW_SECONDS: i64 = 60 * 60;

/// Where a send is. `PendingSend` is the cancel window: nothing has reached
/// Graph yet, so cancelling it is free and final.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendState {
    PendingSend,
    Sending,
    Sent,
    Cancelled,
    Failed,
}

impl SendState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PendingSend => "pending_send",
            Self::Sending => "sending",
            Self::Sent => "sent",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pending_send" => Some(Self::PendingSend),
            "sending" => Some(Self::Sending),
            "sent" => Some(Self::Sent),
            "cancelled" => Some(Self::Cancelled),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }

    /// The state the API reports. The window and the POST both read as
    /// `pending`: the caller learns only what matters to it.
    pub fn public(self) -> &'static str {
        match self {
            Self::PendingSend | Self::Sending => "pending",
            Self::Sent => "sent",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
}

/// One attachment of the draft as the approval sees it. `sha256` is known for
/// the files Excubia attached; a file added in Outlook has none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendAttachment {
    pub name: String,
    pub size: u64,
    pub sha256: Option<String>,
}

/// The draft as the operator reads it before approving: every covered field,
/// the body as the owner wrote it and its content type (`text` or `html`). Reading
/// it changes nothing; a send is the only thing that does. `text()` is the
/// readable form, and the digest covers `body` itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftReview {
    pub account: String,
    pub draft_id: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub content_type: String,
    pub attachments: Vec<SendAttachment>,
}

impl DraftReview {
    /// The body as the operator reads it: plain text, whatever Graph stores.
    pub fn text(&self) -> String {
        text_of(&self.body, &self.content_type)
    }
}

/// The draft at the moment the owner approves it. Its digest is what the
/// approval binds: one byte of change in any covered field changes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendView {
    pub account: String,
    pub draft_id: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body_sha256: String,
    pub body_preview: String,
    pub attachments: Vec<SendAttachment>,
}

impl SendView {
    /// `sha256:<hex>` over the canonical JSON of every covered field. The body
    /// enters as its own digest, so the full text is covered without being
    /// stored in the approval.
    pub fn digest(&self) -> String {
        let attachments: Vec<Value> = self
            .attachments
            .iter()
            .map(|item| json!({"name": item.name, "size": item.size, "sha256": item.sha256}))
            .collect();
        let canonical: BTreeMap<&str, Value> = BTreeMap::from([
            ("account", json!(self.account)),
            ("attachments", json!(attachments)),
            ("body_sha256", json!(self.body_sha256)),
            ("cc", json!(self.cc)),
            ("draft", json!(self.draft_id)),
            ("from", json!(self.from)),
            ("subject", json!(self.subject)),
            ("to", json!(self.to)),
        ]);
        let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
        format!("sha256:{}", digest_hex(&bytes))
    }

    /// What the owner reads before approving: the account, the covered fields,
    /// the start of the body and each attachment's SHA-256 (`null` for a file
    /// added outside Excubia). The host shows it to a device with `APPROVE` only.
    pub fn details(&self) -> Value {
        let attachments: Vec<Value> = self
            .attachments
            .iter()
            .map(|item| json!({"name": item.name, "size": item.size, "sha256": item.sha256}))
            .collect();
        json!({
            "account": self.account,
            "from": self.from,
            "to": self.to,
            "cc": self.cc,
            "subject": self.subject,
            "body_preview": self.body_preview,
            "attachments": attachments,
        })
    }

    /// The domain of each recipient, `to` first. Audit keeps only these.
    pub fn recipient_addresses(&self) -> Vec<String> {
        self.to.iter().chain(self.cc.iter()).cloned().collect()
    }
}

/// The body start the approval shows: the first characters, never the whole.
pub fn preview_of(body: &str) -> String {
    body.chars().take(PREVIEW_CHARS).collect()
}

/// Refuses a send when `per_hour` sends already fall in the window. `sent_at`
/// holds the creation times of the sends that count (not the cancelled ones).
/// The error says how many seconds until a slot frees.
pub fn check_budget(sent_at: &[i64], now: i64, per_hour: u32) -> Result<(), MailError> {
    let window_start = now - RATE_WINDOW_SECONDS;
    let mut recent: Vec<i64> = sent_at
        .iter()
        .copied()
        .filter(|at| *at > window_start)
        .collect();
    let limit = usize::try_from(per_hour).unwrap_or(usize::MAX);
    if recent.len() < limit {
        return Ok(());
    }
    // The slot frees when the send that leaves the window makes room: with
    // `len` sends and a limit of `limit`, that is the one at index `len - limit`.
    recent.sort_unstable();
    let free_at = recent
        .get(recent.len() - limit)
        .map_or(now, |at| at + RATE_WINDOW_SECONDS);
    let retry_after = u64::try_from((free_at - now).max(1)).unwrap_or(1);
    Err(MailError::SendRateLimited { retry_after })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> SendView {
        SendView {
            account: "work".to_owned(),
            draft_id: "AAMk-1".to_owned(),
            from: "owner@example.com".to_owned(),
            to: vec!["ana@example.org".to_owned()],
            cc: Vec::new(),
            subject: "Hola".to_owned(),
            body_sha256: "b".repeat(64),
            body_preview: "Hola".to_owned(),
            attachments: vec![SendAttachment {
                name: "a.pdf".to_owned(),
                size: 3,
                sha256: Some("c".repeat(64)),
            }],
        }
    }

    #[test]
    fn digest_is_stable_and_changes_with_one_covered_field() {
        let base = view();
        assert_eq!(base.digest(), view().digest());

        let mut subject = view();
        subject.subject.push('.');
        let mut body = view();
        body.body_sha256 = "d".repeat(64);
        let mut recipient = view();
        recipient.to[0] = "ana@example.net".to_owned();
        let mut size = view();
        size.attachments[0].size = 4;

        for changed in [subject, body, recipient, size] {
            assert_ne!(base.digest(), changed.digest());
        }
    }

    #[test]
    fn preview_is_bounded_in_characters() {
        let long = "ñ".repeat(PREVIEW_CHARS + 5);
        assert_eq!(preview_of(&long).chars().count(), PREVIEW_CHARS);
    }

    #[test]
    fn budget_refuses_at_the_limit_with_the_wait() {
        let now = 10_000;
        let sent = [now - 100, now - 50, now - 10];
        assert!(check_budget(&sent, now, 4).is_ok());
        match check_budget(&sent, now, 3) {
            Err(MailError::SendRateLimited { retry_after }) => {
                assert_eq!(retry_after, (now - 100 + RATE_WINDOW_SECONDS - now) as u64);
            }
            other => panic!("expected a rate limit, got {other:?}"),
        }
    }

    #[test]
    fn budget_ignores_sends_outside_the_window() {
        let now = 10_000;
        let old = [now - RATE_WINDOW_SECONDS - 1, now - RATE_WINDOW_SECONDS - 2];
        assert!(check_budget(&old, now, 1).is_ok());
    }

    #[test]
    fn states_map_to_the_public_words() {
        assert_eq!(SendState::PendingSend.public(), "pending");
        assert_eq!(SendState::Sending.public(), "pending");
        assert_eq!(SendState::Sent.public(), "sent");
        assert_eq!(SendState::parse("cancelled"), Some(SendState::Cancelled));
        assert_eq!(SendState::parse("nope"), None);
    }
}
