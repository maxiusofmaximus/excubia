//! The body of a draft as the host sends it to Graph: the owner's text and HTML are
//! cleaned, an agent's template is drawn by the host and reaches Graph as it was drawn.

use hn_core_mail::{
    agent_html, sanitize_html, sanitize_text, DraftBody, DraftInput, MailError, Policy,
};
use hn_core_seat::{BodyFormat, DraftFields};

/// Where an HTML body comes from. The owner's HTML is cleaned; an agent template is
/// the host's own markup and reaches Graph as it was drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HtmlOrigin {
    Owner,
    AgentTemplate,
}

pub(crate) fn input(fields: &DraftFields, origin: HtmlOrigin) -> Result<DraftInput, MailError> {
    let subject = sanitize_text(&fields.subject, Policy::Outgoing)?;
    let body = match fields.format {
        BodyFormat::Text => DraftBody::Text(sanitize_text(&fields.body, Policy::Outgoing)?.text),
        BodyFormat::Html => DraftBody::Html(match origin {
            HtmlOrigin::Owner => sanitize_html(&fields.body)?,
            HtmlOrigin::AgentTemplate => fields.body.clone(),
        }),
        BodyFormat::Agent => DraftBody::Html(agent_html(&fields.subject, &fields.body)?),
    };
    Ok(DraftInput {
        to: fields.to.clone(),
        cc: fields.cc.clone(),
        subject: subject.text,
        body,
    })
}
