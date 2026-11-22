//! The fixed layout of an agent's mail. The agent writes a subject and plain
//! text; this module cleans both, escapes them for HTML and fills the one
//! template in `agent.mjml`, rendered by `mrml` (MIT). The agent never supplies
//! markup, and the template includes nothing from outside (no `mj-include`,
//! images, fonts or scripts).

use crate::error::MailError;
use crate::markup::{html_to_text, HTML_CONTENT_TYPE};
use crate::redact::Redacted;
use crate::sanitize::{sanitize_text, Policy};

const TEMPLATE: &str = include_str!("agent.mjml");
const SUBJECT_MARK: &str = "{{SUBJECT}}";
const BODY_MARK: &str = "{{BODY}}";
/// Wraps the rendered body. Escaped agent text can never produce these, so a
/// body that spells them stays text.
const OPEN_MARK: &str = "<!--excubia:agent:v1-->";
const CLOSE_MARK: &str = "<!--/excubia:agent:v1-->";
const MAX_PARAGRAPHS: usize = 200;
const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

/// The HTML of an agent draft: the subject as its title and the body as
/// paragraphs. Both texts are cleaned by the same policy as any owner text.
pub fn agent_html(subject: &str, body: &str) -> Result<String, MailError> {
    let subject = escape(&sanitize_text(subject, Policy::Outgoing)?.text);
    let body = paragraphs(&sanitize_text(body, Policy::Outgoing)?.text)?;
    let html = render(&fill(TEMPLATE, &subject, &body))?;
    if html.len() > MAX_HTML_BYTES {
        return Err(MailError::InvalidInput(Redacted::new(
            "the rendered mail is larger than 2 MiB",
        )));
    }
    Ok(html)
}

/// The text an agent draft was written from, read back from between its marks
/// with the same conversion the preview uses. `None` for any other body: a draft
/// without the marks keeps its HTML as it is.
pub fn agent_text(content: &str, content_type: &str) -> Option<String> {
    if !content_type.eq_ignore_ascii_case(HTML_CONTENT_TYPE) {
        return None;
    }
    let (_, rest) = content.split_once(OPEN_MARK)?;
    let (inside, _) = rest.split_once(CLOSE_MARK)?;
    Some(html_to_text(inside))
}

/// One pass over the template: the inserted text is never scanned for markers,
/// so a body that spells `{{SUBJECT}}` stays text.
fn fill(template: &str, subject: &str, body: &str) -> String {
    let mut out = String::with_capacity(template.len() + subject.len() + body.len());
    let mut rest = template;
    while let Some(at) = rest.find("{{") {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if let Some(after) = tail.strip_prefix(SUBJECT_MARK) {
            out.push_str(subject);
            rest = after;
        } else if let Some(after) = tail.strip_prefix(BODY_MARK) {
            out.push_str(body);
            rest = after;
        } else {
            out.push_str("{{");
            rest = &tail[2..];
        }
    }
    out.push_str(rest);
    out
}

/// Blank lines separate paragraphs; a single line break inside one is kept as
/// `<br>`. A blank line is a pair of `<br>` between two paragraphs of one
/// `mj-text`, so `agent_text` reads back the blank lines the agent wrote.
fn paragraphs(text: &str) -> Result<String, MailError> {
    let lines: Vec<&str> = text.split('\n').collect();
    let blocks: Vec<&[&str]> = lines
        .chunk_by(|a, b| is_blank(a) == is_blank(b))
        .filter(|block| !is_blank(block[0]))
        .collect();
    if blocks.len() > MAX_PARAGRAPHS {
        return Err(MailError::InvalidInput(Redacted::new(
            "the body has more than 200 paragraphs",
        )));
    }
    let paragraphs: Vec<String> = blocks
        .iter()
        .map(|block| {
            let lines: Vec<String> = block.iter().map(|line| escape(line)).collect();
            lines.join("<br>")
        })
        .collect();
    Ok(format!(
        "<mj-text>{}</mj-text>",
        paragraphs.join("<br><br>")
    ))
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn render(source: &str) -> Result<String, MailError> {
    let refused =
        || MailError::InvalidInput(Redacted::new("the mail template could not be rendered"));
    let parsed = mrml::parse(source).map_err(|_| refused())?;
    parsed
        .element
        .render(&mrml::prelude::render::RenderOptions::default())
        .map_err(|_| refused())
}
