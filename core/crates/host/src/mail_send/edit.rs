//! The owner's own edit of a draft (MAILUX-HOST §B), and the chat card of a draft
//! (§A). The owner's fields pass the same checks the tools apply; the audit keeps
//! the device and never the content, and the cards hold no body.

use hn_core_api::DraftEdit;
use hn_core_chat::MailDraftCard;
use hn_core_mail::{
    agent_text, DraftBody, DraftReview, EditRecord, GraphClient, MailError, Redacted,
};
use hn_core_seat::check_draft_fields;

use super::graph::{owner_view, sender};
use super::MailSends;
use crate::mail_draft_input::{input, HtmlOrigin};

/// The longest preview a card shows, in characters.
const PREVIEW_CHARS: usize = 160;

/// The body an edit is checked as, with the format it is checked in and where its
/// HTML came from. An agent draft keeps its HTML unless its text or its subject
/// changes; then the template is drawn again from the owner's text. Any other
/// untouched body goes back as Graph holds it. An untouched agent template is the
/// host's markup, so it is not cleaned as the owner's HTML would be.
fn edited_body(
    current: &DraftReview,
    body: Option<String>,
    subject: &str,
) -> (String, String, HtmlOrigin) {
    match (body, agent_text(&current.body, &current.content_type)) {
        (Some(text), Some(_)) => (text, "agent".to_owned(), HtmlOrigin::Owner),
        (Some(text), None) => (text, "text".to_owned(), HtmlOrigin::Owner),
        (None, Some(text)) if subject != current.subject => {
            (text, "agent".to_owned(), HtmlOrigin::Owner)
        }
        (None, Some(_)) => (
            current.body.clone(),
            current.content_type.clone(),
            HtmlOrigin::AgentTemplate,
        ),
        (None, None) => (
            current.body.clone(),
            current.content_type.clone(),
            HtmlOrigin::Owner,
        ),
    }
}

impl MailSends {
    /// Applies the owner's edit to a draft this account created. A field left out
    /// keeps its value. The answer is the draft as the review serves it.
    pub async fn update_draft(
        &self,
        account: &str,
        draft_id: &str,
        edit: DraftEdit,
        device_id: &str,
    ) -> Result<DraftReview, MailError> {
        let saved = self.saved(account)?;
        let access = self.login_access(&saved, draft_id).await?;
        let client = GraphClient::new(
            self.inner.transport.as_ref(),
            &access,
            &self.inner.ledger,
            &saved.label,
        );
        let current = client.read_for_review(draft_id).await?;
        let subject = edit.subject.unwrap_or_else(|| current.subject.clone());
        let (body, format, origin) = edited_body(&current, edit.body, &subject);
        let fields = check_draft_fields(
            account,
            &edit.to.unwrap_or(current.to),
            &edit.cc.unwrap_or(current.cc),
            &subject,
            &body,
            &format,
        )
        .map_err(|error| MailError::InvalidInput(Redacted::new(error.message)))?;
        let draft = input(&fields, origin)?;
        client.update_draft(draft_id, &draft).await?;
        self.draft_changed(account, draft_id)?;
        self.inner
            .audit
            .append(&EditRecord::new(account, draft_id, device_id))?;
        self.show_draft(
            None,
            account,
            draft_id,
            &draft.to,
            &draft.subject,
            &draft.body,
        )?;
        Ok(owner_view(DraftReview {
            account: account.to_owned(),
            draft_id: draft_id.to_owned(),
            from: sender(current.from, &saved)?,
            to: draft.to,
            cc: draft.cc,
            subject: draft.subject,
            body: draft.body.content().to_owned(),
            content_type: draft.body.content_type().to_owned(),
            attachments: current.attachments,
        }))
    }

    /// Writes the chat card of a draft the conversation can see, or, with no
    /// conversation, refreshes the cards that already show it.
    pub(crate) fn show_draft(
        &self,
        conversation: Option<&str>,
        account: &str,
        draft_id: &str,
        to: &[String],
        subject: &str,
        body: &DraftBody,
    ) -> Result<(), MailError> {
        let preview = card_preview(&body.text());
        self.inner
            .chat
            .put_mail_draft_card(
                conversation,
                MailDraftCard {
                    account,
                    draft_id,
                    to,
                    subject,
                    preview: &preview,
                },
            )
            .map_err(|error| MailError::Store(Redacted::new(error.to_string())))
    }
}

/// The body in one line, cut to the card's room.
fn card_preview(body: &str) -> String {
    body.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(PREVIEW_CHARS)
        .collect()
}
