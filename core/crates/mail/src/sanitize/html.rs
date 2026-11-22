//! The HTML of an owner's own draft: the cleaning `sanitize_text` gives any text,
//! then an allowlist parser (ammonia) that keeps only the markup of the list. The
//! agent's template never comes here; it is the host's own markup.

use std::collections::{HashMap, HashSet};

use ammonia::{Builder, UrlRelative};

use super::{clean, is_visible, rejected, Policy};
use crate::error::MailError;

/// The most bytes an HTML body may have before anything is scanned.
const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;
/// The deepest element nesting an HTML body may have.
const MAX_DEPTH: usize = 100;
/// The most tags an HTML body may have.
const MAX_TAGS: usize = 20_000;

const TAGS: &[&str] = &[
    "p",
    "br",
    "div",
    "span",
    "strong",
    "b",
    "em",
    "i",
    "u",
    "s",
    "ul",
    "ol",
    "li",
    "blockquote",
    "pre",
    "code",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "table",
    "thead",
    "tbody",
    "tr",
    "th",
    "td",
    "a",
    "hr",
];
/// Their whole subtree goes, not only the tag.
const CONTENT_TAGS: &[&str] = &[
    "script", "style", "head", "title", "noscript", "template", "iframe", "object", "embed",
    "form", "input", "button", "svg", "math", "video", "audio", "canvas",
];
const GENERIC_ATTRIBUTES: &[&str] = &["lang", "dir", "title"];
const URL_SCHEMES: &[&str] = &["http", "https", "mailto"];
const LINK_REL: &str = "noopener noreferrer nofollow";
/// Elements with no end tag: they never nest, so the depth count skips them.
const VOID_TAGS: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "img", "input",
    "keygen", "link", "meta", "param", "source", "track", "wbr",
];

/// Cleans the HTML of an owner's draft. The size and shape are checked before the
/// parser runs, the text is cleaned as any text is, and only the allowlist survives.
pub fn sanitize_html(input: &str) -> Result<String, MailError> {
    if input.is_empty() {
        return Ok(String::new());
    }
    if input.len() > MAX_HTML_BYTES {
        return Err(rejected("the HTML is larger than 2 MiB"));
    }
    check_shape(input)?;
    let (cleaned, _) = clean(input, Policy::Outgoing)?;
    let html = policy().clean(&cleaned).to_string();
    if !has_visible_text(&html) {
        return Err(rejected("the text has no visible content"));
    }
    Ok(html)
}

/// The Outgoing allowlist, set field by field so nothing comes from the defaults.
fn policy() -> Builder<'static> {
    let mut builder = Builder::empty();
    builder
        .tags(set(TAGS))
        .clean_content_tags(set(CONTENT_TAGS))
        .generic_attributes(set(GENERIC_ATTRIBUTES))
        .tag_attributes(HashMap::from([
            ("a", set(&["href", "title"])),
            ("td", set(&["colspan", "rowspan"])),
            ("th", set(&["colspan", "rowspan"])),
        ]))
        .url_schemes(set(URL_SCHEMES))
        .url_relative(UrlRelative::Deny)
        .link_rel(Some(LINK_REL))
        .set_tag_attribute_value("a", "referrerpolicy", "no-referrer")
        .strip_comments(true);
    builder
}

fn set(items: &[&'static str]) -> HashSet<&'static str> {
    items.iter().copied().collect()
}

/// The text a reader sees: the markup is dropped and what is left is judged as
/// `sanitize_text` judges text.
fn has_visible_text(html: &str) -> bool {
    Builder::empty()
        .clean(html)
        .to_string()
        .chars()
        .any(is_visible)
}

/// Refuses a body that is too deep or has too many tags before the parser sees it.
/// shortcut: this counts the markup, not the parser's tree: unclosed p, li and td
/// count as nested, and tags inside attribute values are not skipped; upgrade to
/// the parser's own open-element stack if real mail trips the limit.
fn check_shape(input: &str) -> Result<(), MailError> {
    let mut open: Vec<&str> = Vec::new();
    let mut tags = 0;
    let mut rest = input;
    while let Some(at) = rest.find('<') {
        rest = &rest[at + 1..];
        if let Some(comment) = rest.strip_prefix("!--") {
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
            continue;
        }
        let Some((closing, name)) = tag_at(rest) else {
            continue;
        };
        tags += 1;
        if tags > MAX_TAGS {
            return Err(rejected("the HTML has more than 20000 tags"));
        }
        if closing {
            if open
                .last()
                .is_some_and(|top| top.eq_ignore_ascii_case(name))
            {
                open.pop();
            }
        } else if !VOID_TAGS.iter().any(|void| void.eq_ignore_ascii_case(name)) {
            open.push(name);
            if open.len() > MAX_DEPTH {
                return Err(rejected("the HTML nests more than 100 levels"));
            }
        }
    }
    Ok(())
}

/// The name of the tag that starts right after a `<`, and whether it is an end tag.
/// The name ends where the tokenizer ends it: at whitespace, `/` or `>`.
fn tag_at(after: &str) -> Option<(bool, &str)> {
    let (closing, body) = match after.strip_prefix('/') {
        Some(body) => (true, body),
        None => (false, after),
    };
    if !body.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
        return None;
    }
    let len = body
        .find(['\t', '\n', '\u{c}', '\r', ' ', '/', '>'])
        .unwrap_or(body.len());
    Some((closing, &body[..len]))
}
