//! What a received mail says about itself, checked by the host (MAIL-LECTURA).
//!
//! Everything here is a pure function of the headers and the body Graph gave:
//! the same mail always gets the same checks, so the model reports a verdict the
//! host computed and never works one out from raw text. Nothing is fetched and
//! no link is followed.

use serde::Serialize;

use crate::audit::domain_of;
use crate::markup::{html_to_text, text_of};

/// The most characters of a body a reader gets; a longer one is cut.
pub const MAX_BODY_CHARS: usize = 12_000;
/// The most links a reader gets. Every link still counts for `link_host_differs`.
pub const MAX_LINKS: usize = 20;
const MAX_LINK_TEXT_CHARS: usize = 80;
const MAX_HEADER_CHARS: usize = 1_000;
const CLAIMS: [&str; 5] = ["adjunto", "adjunta", "adjuntamos", "attached", "attachment"];

/// The SPF, DKIM and DMARC verdicts. `missing` means the mail names no result for
/// the method, or names one this list does not know: there is nothing to check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthSummary {
    pub spf: &'static str,
    pub dkim: &'static str,
    pub dmarc: &'static str,
}

/// One `http` or `https` link of the body: the text a reader sees and the host the
/// `href` really points to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    pub text: String,
    pub host: String,
    /// The host the visible text names, when it reads like a domain. Not returned.
    #[serde(skip)]
    pub text_host: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttachmentInfo {
    pub name: String,
    pub content_type: String,
    pub size: u64,
    pub is_inline: bool,
}

/// The verdicts `checks` returns. Domains are lowercase; `None` means the mail
/// has no such address or header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Checks {
    pub from_domain: Option<String>,
    pub reply_to_domain: Option<String>,
    pub return_path_domain: Option<String>,
    pub reply_to_differs: bool,
    pub return_path_differs: bool,
    pub auth: AuthSummary,
    pub claims_attachment_but_none: bool,
    pub link_host_differs: bool,
    pub list_unsubscribe: bool,
    pub one_click_unsubscribe: bool,
    pub bulk: bool,
}

/// The headers a reader gets, each only when the mail has it and at most 1 000
/// characters. Nothing else of the header block leaves the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeptHeaders {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication_results: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received_spf: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_mailer: Option<String>,
}

/// The values of every header called `name`, in the order Graph gave them.
pub fn header_values<'a>(pairs: &'a [(String, String)], name: &str) -> Vec<&'a str> {
    pairs
        .iter()
        .filter(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .collect()
}

pub fn kept_headers(pairs: &[(String, String)]) -> KeptHeaders {
    let first = |name: &str| {
        header_values(pairs, name)
            .first()
            .map(|value| one_line(value, MAX_HEADER_CHARS))
    };
    KeptHeaders {
        return_path: first("Return-Path"),
        reply_to: first("Reply-To"),
        authentication_results: first("Authentication-Results"),
        received_spf: first("Received-SPF"),
        message_id: first("Message-ID"),
        x_mailer: first("X-Mailer"),
    }
}

/// The verdict of SPF, DKIM and DMARC. The topmost header that names a method
/// decides it; `Received-SPF` is the fallback for SPF only.
pub fn auth_summary(auth_results: &[&str], received_spf: Option<&str>) -> AuthSummary {
    let spf = auth_results
        .iter()
        .find_map(|header| result_of(header, "spf"))
        .or_else(|| received_spf.and_then(leading_word));
    AuthSummary {
        spf: verdict(spf),
        dkim: verdict(
            auth_results
                .iter()
                .find_map(|header| result_of(header, "dkim")),
        ),
        dmarc: verdict(
            auth_results
                .iter()
                .find_map(|header| result_of(header, "dmarc")),
        ),
    }
}

fn verdict(word: Option<String>) -> &'static str {
    match word.as_deref() {
        Some("pass") => "pass",
        Some("fail") => "fail",
        Some("softfail") => "softfail",
        Some("neutral") => "neutral",
        Some("none") => "none",
        Some("temperror") => "temperror",
        Some("permerror") => "permerror",
        _ => "missing",
    }
}

/// The result word after the first `method=` that starts a tag, so `dkim=` does
/// not match inside `x-dkim=`.
fn result_of(header: &str, method: &str) -> Option<String> {
    let lower = header.to_ascii_lowercase();
    let needle = format!("{method}=");
    let mut from = 0;
    while let Some(at) = lower[from..].find(&needle) {
        let start = from + at;
        let starts_tag = lower[..start]
            .chars()
            .next_back()
            .is_none_or(|ch| ch == ';' || ch.is_whitespace());
        if starts_tag {
            return leading_word(&lower[start + needle.len()..]);
        }
        from = start + needle.len();
    }
    None
}

/// The letters at the start of `text`, lowercase: `pass (google.com: ...)` gives `pass`.
fn leading_word(text: &str) -> Option<String> {
    let word: String = text
        .trim_start()
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect();
    Some(word.to_ascii_lowercase())
}

/// The `http` and `https` links of an HTML body, in order. Other schemes are dropped.
pub fn links(html: &str) -> Vec<Link> {
    let lower = html.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = lower[at..].find("<a") {
        let open = at + offset;
        let name_end = lower.as_bytes().get(open + 2).copied();
        at = open + 2;
        if !matches!(name_end, Some(b' ' | b'\t' | b'\r' | b'\n' | b'>')) {
            continue;
        }
        let Some(close_tag) = lower[open..].find('>') else {
            break;
        };
        let tag_end = open + close_tag;
        let close = lower[tag_end..]
            .find("</a")
            .map_or(html.len(), |offset| tag_end + offset);
        at = close;
        let Some(href) = href_of(&html[open..tag_end]) else {
            continue;
        };
        let Some(host) = http_host(href) else {
            continue;
        };
        let text = html_to_text(&html[tag_end + 1..close]);
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        found.push(Link {
            text_host: text_host(&text),
            text: cut(&text, MAX_LINK_TEXT_CHARS).0,
            host,
        });
    }
    found
}

/// The value of the `href` attribute of one `<a ...>` tag, quoted or bare.
fn href_of(tag: &str) -> Option<&str> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    let at = loop {
        let found = from + lower[from..].find("href")?;
        if lower[..found]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
        {
            break found;
        }
        from = found + 4;
    };
    let rest = tag[at + 4..].trim_start().strip_prefix('=')?.trim_start();
    match rest.chars().next()? {
        quote @ ('"' | '\'') => rest[1..].split(quote).next(),
        _ => rest.split(char::is_whitespace).next(),
    }
}

/// The host of an `http` or `https` address, lowercase, without user, port or dot.
fn http_host(href: &str) -> Option<String> {
    let lower = href.trim().to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority
        .rsplit('@')
        .next()?
        .split(':')
        .next()?
        .trim_end_matches('.');
    (!host.is_empty()).then(|| host.to_owned())
}

/// The host a link's visible text names, when the text reads like a domain or a URL.
fn text_host(text: &str) -> Option<String> {
    let lower = text.trim().to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(lower.as_str());
    let host = rest
        .split(['/', '?', '#', ' '])
        .next()?
        .trim_end_matches('.');
    let tld = host.rsplit('.').next()?;
    let domain_like = host.contains('.')
        && tld.len() >= 2
        && tld.chars().all(|ch| ch.is_ascii_alphabetic())
        && host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-');
    domain_like.then(|| host.to_owned())
}

/// Whether the text of a link names a host other than the one it points to.
/// shortcut: compares whole hosts without `www.`, not registrable domains (no
/// public-suffix list), so `mail.banco.es` shown as `banco.es` counts as different;
/// upgrade with a public-suffix list.
fn link_text_differs(link: &Link) -> bool {
    link.text_host
        .as_deref()
        .is_some_and(|shown| bare(shown) != bare(&link.host))
}

fn bare(host: &str) -> &str {
    host.strip_prefix("www.").unwrap_or(host)
}

/// Whether the text says an attachment is there.
pub fn claims_attachment(text: &str) -> bool {
    let lower = text.to_lowercase();
    CLAIMS.iter().any(|word| lower.contains(word))
}

/// The checks of one received mail. `reply_to` is the addresses the mail names,
/// `headers` every header of it as Graph gave them, and `text` the whole body as a
/// reader sees it.
pub fn check_message(
    from: &str,
    reply_to: &[String],
    headers: &[(String, String)],
    auth: AuthSummary,
    text: &str,
    attachments: &[AttachmentInfo],
    links: &[Link],
) -> Checks {
    let from_domain = address_domain(from);
    let reply_to_domains: Vec<String> = reply_to
        .iter()
        .filter_map(|address| address_domain(address))
        .collect();
    let return_path_domain = header_values(headers, "Return-Path")
        .first()
        .and_then(|value| address_domain(value.trim().trim_matches(['<', '>'])));
    let list_unsubscribe = !header_values(headers, "List-Unsubscribe").is_empty();
    let one_click_unsubscribe =
        header_values(headers, "List-Unsubscribe-Post")
            .iter()
            .any(|value| {
                value
                    .to_ascii_lowercase()
                    .contains("list-unsubscribe=one-click")
            });
    let list_precedence = header_values(headers, "Precedence").iter().any(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "bulk" | "list" | "junk"
        )
    });
    // shortcut: compares whole domains, not registrable ones (no public-suffix list), so a
    // reply to sub.x.com from x.com counts as different; upgrade with a public-suffix list.
    let reply_to_differs = from_domain
        .as_ref()
        .is_some_and(|from| reply_to_domains.iter().any(|domain| domain != from));
    let return_path_differs = match (&from_domain, &return_path_domain) {
        (Some(from), Some(path)) => from != path,
        _ => false,
    };
    Checks {
        from_domain,
        reply_to_domain: reply_to_domains.first().cloned(),
        return_path_domain,
        reply_to_differs,
        return_path_differs,
        auth,
        claims_attachment_but_none: claims_attachment(text)
            && attachments.iter().all(|attachment| attachment.is_inline),
        link_host_differs: links.iter().any(link_text_differs),
        list_unsubscribe,
        one_click_unsubscribe,
        bulk: list_unsubscribe || list_precedence,
    }
}

fn address_domain(address: &str) -> Option<String> {
    let domain = domain_of(address);
    (!domain.is_empty()).then_some(domain)
}

/// The text a reader sees: HTML turned into lines, then every control character
/// but the line breaks and tabs removed.
pub fn clean_body(content: &str, content_type: &str) -> String {
    text_of(content, content_type)
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'))
        .collect()
}

/// The first `max` characters of `text`, and whether anything was cut.
pub fn cut(text: &str, max: usize) -> (String, bool) {
    (text.chars().take(max).collect(), text.chars().count() > max)
}

/// One line of text: control characters become spaces, and at most `max` characters stay.
pub fn one_line(raw: &str, max: usize) -> String {
    raw.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .take(max)
        .collect()
}
