//! The Graph side of a send (MAIL2A): reading the draft the owner approved or
//! reviews, refreshing the send token and the one POST that sends. It lives apart
//! from `mail_send` only to keep each file under the size rule; the state is there.

use excubia_core_mail::auth::{refresh, refresh_send};
use excubia_core_mail::{
    agent_text, Account, DraftReview, GraphClient, MailError, Redacted, SendFailure, SendRow,
    SendView,
};

use super::sweep::report;
use super::MailSends;

impl MailSends {
    /// The draft as the send sees it: the one read the approval binds.
    pub(super) async fn read_draft(
        &self,
        saved: &Account,
        draft_id: &str,
    ) -> Result<SendView, MailError> {
        let access = self.login_access(saved, draft_id).await?;
        let mut view = GraphClient::new(
            self.inner.transport.as_ref(),
            &access,
            &self.inner.ledger,
            &saved.label,
        )
        .read_for_send(draft_id)
        .await?;
        view.from = sender(view.from, saved)?;
        Ok(view)
    }

    /// The draft in full, for the operator to read before approving. Reads only.
    pub(super) async fn review_draft(
        &self,
        saved: &Account,
        draft_id: &str,
    ) -> Result<DraftReview, MailError> {
        let access = self.login_access(saved, draft_id).await?;
        let mut review = GraphClient::new(
            self.inner.transport.as_ref(),
            &access,
            &self.inner.ledger,
            &saved.label,
        )
        .read_for_review(draft_id)
        .await?;
        review.from = sender(review.from, saved)?;
        Ok(owner_view(review))
    }

    /// Refuses a draft this account did not create before Microsoft is asked
    /// anything. Then refreshes the login token and saves the rotated pair.
    pub(super) async fn login_access(
        &self,
        saved: &Account,
        draft_id: &str,
    ) -> Result<String, MailError> {
        if !self.inner.ledger.owns(&saved.label, draft_id)? {
            return Err(MailError::DraftNotOwned {
                id: Redacted::new(draft_id),
            });
        }
        let client_id = self.client_id()?;
        let refresh_token = saved
            .tokens
            .refresh_token()
            .ok_or(MailError::AuthRequired)?;
        let tokens = refresh(
            self.inner.transport.as_ref(),
            client_id,
            saved.tenant,
            refresh_token,
        )
        .await?;
        let access = tokens.access_token().to_owned();
        self.inner.store.save(&saved.renewed(tokens))?;
        Ok(access)
    }

    /// The Graph side of a send: the draft must still match the digest, and then
    /// one POST with the send token. Anything else is a failure, nothing more.
    pub(super) async fn dispatch(&self, row: &SendRow) -> Result<(), SendFailure> {
        let saved = match self.saved(&row.account) {
            Ok(saved) => saved,
            Err(error) => return Err(failed("the account could not be read", &error, false)),
        };
        let view = match self.read_draft(&saved, &row.draft_id).await {
            Ok(view) => view,
            Err(error) => return Err(failed("the draft could not be read", &error, false)),
        };
        if view.digest() != row.digest {
            return Err(SendFailure::DRAFT_CHANGED);
        }
        let access = match self.send_token(&row.account).await {
            Ok(access) => access,
            Err(error) => {
                return Err(failed(
                    "the send token could not be refreshed",
                    &error,
                    false,
                ))
            }
        };
        let transport = self.inner.transport.as_ref();
        GraphClient::new(transport, &access, &self.inner.ledger, &row.account)
            .send_draft(&row.draft_id)
            .await
            .map_err(|error| failed("Graph refused the send", &error, true))
    }

    /// Refreshes the send token and saves the rotated pair. `SendNotEnabled` when
    /// the owner has revoked the consent since the request.
    pub(super) async fn send_token(&self, account: &str) -> Result<String, MailError> {
        let saved = self.saved(account)?;
        let client_id = self.client_id()?;
        let send = saved.send.as_ref().ok_or(MailError::SendNotEnabled)?;
        let refresh_token = send.refresh_token().ok_or(MailError::AuthRequired)?;
        let tokens = refresh_send(
            self.inner.transport.as_ref(),
            client_id,
            saved.tenant,
            refresh_token,
        )
        .await?;
        let access = tokens.access_token().to_owned();
        self.inner.store.save(&saved.with_send(tokens))?;
        Ok(access)
    }
}

/// A send that did not leave: the step and the error code reach the host's log,
/// never the detail (`report` prints the code only). `posted` is whether the POST
/// to Graph went out, which decides whether the failure may already have sent it.
fn failed(step: &str, error: &MailError, posted: bool) -> SendFailure {
    report(step, error);
    SendFailure::of(error, posted)
}

/// The draft as the owner reads and edits it. An agent draft shows the text the
/// agent wrote, as `text`, the shape every plain draft has; any other draft keeps
/// its body as Graph holds it. The approval digests the stored body, not this view.
pub(super) fn owner_view(mut review: DraftReview) -> DraftReview {
    if let Some(text) = agent_text(&review.body, &review.content_type) {
        review.body = text;
        review.content_type = "text".to_owned();
    }
    review
}

/// The sender of a draft: the address Graph reports, or the account's own address
/// when Graph reports none (MAILUX-HOST §D). It is never empty.
pub(super) fn sender(reported: String, saved: &Account) -> Result<String, MailError> {
    if !reported.is_empty() {
        return Ok(reported);
    }
    if saved.upn.is_empty() {
        return Err(MailError::BadResponse(Redacted::new(
            "the draft names no sender and the account has no address",
        )));
    }
    Ok(saved.upn.clone())
}
