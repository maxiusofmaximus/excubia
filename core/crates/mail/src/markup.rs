//! The text of a draft body. Graph stores a body as `text` or as `html`; the
//! operator always reads text, so HTML is turned into its lines here.
//!
//! Any HTML is turned into its lines: tags are dropped and entities decoded,
//! nothing more.

/// The content type Graph stores for a plain-text body, kept as written.
pub const TEXT_CONTENT_TYPE: &str = "text";

/// The content type Graph stores for an HTML body.
pub const HTML_CONTENT_TYPE: &str = "html";

/// The visible text of a body. `text/plain` bodies are already text; HTML is
/// converted by `html_to_text`.
pub fn text_of(content: &str, content_type: &str) -> String {
    if content_type.eq_ignore_ascii_case(HTML_CONTENT_TYPE) {
        html_to_text(content)
    } else {
        content.to_owned()
    }
}

pub fn html_to_text(html: &str) -> String {
    let mut plain = Plain::default();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        plain.text(&rest[..start]);
        let tail = &rest[start..];
        let Some(next) = tail.as_bytes().get(1).copied() else {
            plain.text("<");
            break;
        };
        if tail.starts_with("<!--") {
            rest = tail.find("-->").map_or("", |end| &tail[end + 3..]);
            continue;
        }
        if !(next.is_ascii_alphabetic() || matches!(next, b'/' | b'!' | b'?')) {
            // A bare `<`, as in "a < b": text, not a tag.
            plain.text("<");
            rest = &tail[1..];
            continue;
        }
        let Some(end) = tail.find('>') else {
            plain.text(tail);
            break;
        };
        let tag = &tail[1..end];
        rest = &tail[end + 1..];
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|ch: char| ch.is_whitespace() || ch == '/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match name.as_str() {
            "br" => plain.line_break(),
            "div" | "p" | "table" | "tr" | "td" | "th" | "h1" | "h2" | "h3" | "li" => {
                plain.block_edge();
            }
            "head" | "style" | "script" | "title" if !closing => {
                // Not part of the message's text: skip to its closing tag.
                let lower = rest.to_ascii_lowercase();
                rest = lower
                    .find(&format!("</{name}"))
                    .map_or(rest, |at| &rest[at..]);
            }
            _ => {}
        }
    }
    plain.text(rest);
    plain.finish()
}

/// The text being built. A `<br>` is held back until the next thing shows
/// whether its line ends there or goes on, as in a browser: a break at the very
/// end of a block adds no line of its own.
#[derive(Default)]
struct Plain {
    out: String,
    pending_break: bool,
}

impl Plain {
    fn text(&mut self, raw: &str) {
        // Nothing, or only the line breaks that indent the source between blocks.
        if raw.is_empty() || (raw.trim().is_empty() && raw.contains(['\r', '\n'])) {
            return;
        }
        self.flush();
        // Line breaks in the source are whitespace in HTML, never text.
        self.out.extend(
            decode(raw)
                .chars()
                .map(|ch| if ch == '\n' || ch == '\r' { ' ' } else { ch }),
        );
    }

    fn line_break(&mut self) {
        self.flush();
        self.pending_break = true;
    }

    /// A block opens or closes: its line starts on a new line and ends the
    /// current one.
    fn block_edge(&mut self) {
        self.flush();
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    fn flush(&mut self) {
        if self.pending_break {
            self.pending_break = false;
            self.out.push('\n');
        }
    }

    fn finish(mut self) -> String {
        self.flush();
        // The line break that ends the last block is markup, not text.
        if self.out.ends_with('\n') {
            self.out.pop();
        }
        self.out
    }
}

/// Decodes character references in one pass, so `&amp;lt;` reads `&lt;`.
fn decode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let semicolon = tail
            .char_indices()
            .take(12)
            .find_map(|(index, ch)| (ch == ';').then_some(index));
        match semicolon.and_then(|end| entity(&tail[1..end]).map(|ch| (ch, end))) {
            Some((ch, end)) => {
                out.push(ch);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix(|ch| ch == 'x' || ch == 'X') {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}
