//! The body of a draft as the host sends it to Graph: the owner's text and HTML are
//! cleaned, an agent's template is drawn by the host and reaches Graph as it was drawn.

use excubia_mail::{
    agent_html, sanitize_html, sanitize_text, DraftBody, DraftInput, MailError, Policy,
};
use excubia_seat::{BodyFormat, DraftFields};

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

pub(crate) const ATTACHMENT_WARNING: &str = "El cuerpo menciona un adjunto pero el borrador no lleva ninguno. Usa mail.attach_file o quita la frase.";
/// Words that promise a file in a body; matched on the folded text (see [`fold`]).
const ATTACHMENT_WORDS: [&str; 6] = [
    "adjunto",
    "adjunta",
    "adjuntamos",
    "attached",
    "attachment",
    "te envio el archivo",
];

/// The warning for a body that promises a file the draft does not carry. It only warns:
/// the draft is saved either way.
pub(crate) fn attachment_warning(body: &str, attachments: &[String]) -> Option<&'static str> {
    let folded = fold(body);
    let promises = ATTACHMENT_WORDS.iter().any(|word| folded.contains(word));
    (promises && attachments.is_empty()).then_some(ATTACHMENT_WARNING)
}

/// Lowercase, without the Spanish accents, with every run of spaces or line breaks as one space.
fn fold(text: &str) -> String {
    let lower = text.to_lowercase();
    let plain: String = lower
        .chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            other => other,
        })
        .collect();
    plain.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warns(body: &str, attachments: &[String]) -> bool {
        attachment_warning(body, attachments).is_some()
    }

    #[test]
    fn a_promised_file_without_an_attachment_warns_whatever_the_case_or_accent() {
        assert!(warns("Te envío el archivo.", &[]));
        assert!(warns("ADJÚNTAMOS la factura", &[]));
        assert!(warns("Va un adjunto.", &[]));
        assert!(warns("Please see the attached file.", &[]));
        assert!(warns("Te envío\n   el  archivo hoy", &[]));
    }

    #[test]
    fn a_draft_that_carries_its_file_or_promises_none_does_not_warn() {
        assert!(!warns("Va un adjunto.", &["factura.pdf".to_string()]));
        assert!(!warns("Nos vemos mañana.", &[]));
        assert_eq!(
            attachment_warning("Adjunto la factura.", &[]),
            Some(ATTACHMENT_WARNING)
        );
    }
}
