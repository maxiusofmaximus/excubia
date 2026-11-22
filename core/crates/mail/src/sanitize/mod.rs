//! The text the product sends: one pipeline, two policies, no surprises.
//!
//! `Outgoing` is what the owner or an agent writes; `Incoming` is content from
//! someone else and, among other things, turns every non-ASCII space into a
//! plain space. The order is fixed: byte cap, NUL, line endings, categories
//! (which is also where emoji sequences are validated), NFC, limits, and at
//! last "there must be something visible". A rejected input never carries its
//! own text into the error.

use std::borrow::Cow;
use std::cmp::Ordering;

use unicode_general_category::{get_general_category, GeneralCategory};
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

use crate::error::MailError;
use crate::redact::Redacted;

mod emoji;
mod html;

pub use emoji::UNICODE_VERSION as UNICODE_EMOJI_VERSION;
pub use html::sanitize_html;

/// The most bytes a text body may have before anything is decoded.
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
/// The most characters a text body may have after cleaning.
pub const MAX_TEXT_CHARS: usize = 100_000;
/// The most grapheme clusters a text body may have after cleaning.
pub const MAX_TEXT_GRAPHEMES: usize = 100_000;
/// The most lines a text body may have after cleaning.
pub const MAX_TEXT_LINES: usize = 10_000;
/// The most characters one line may have after cleaning.
pub const MAX_LINE_CHARS: usize = 10_000;
/// The most combining marks one grapheme cluster may carry.
pub const MAX_MARKS_PER_GRAPHEME: usize = 8;

/// Who wrote the text decides how its spaces travel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// The owner or an agent: Unicode spaces, U+00A0 included, are the writer's.
    Outgoing,
    /// Someone else: every non-ASCII space becomes a plain ASCII space.
    Incoming,
}

/// A clean text and how many characters were removed or replaced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sanitized {
    pub text: String,
    pub removed: usize,
}

/// Cleans one text subject or body.
pub fn sanitize_text(input: &str, policy: Policy) -> Result<Sanitized, MailError> {
    // An empty subject or body is the writer's choice; only text that cleans down to nothing is refused.
    if input.is_empty() {
        return Ok(Sanitized {
            text: String::new(),
            removed: 0,
        });
    }
    if input.len() > MAX_TEXT_BYTES {
        return Err(rejected("the text is larger than 1 MiB"));
    }
    let (text, removed) = clean(input, policy)?;
    check_limits(&text)?;
    Ok(Sanitized { text, removed })
}

/// The cleaning every pipeline shares: refuses what no text may carry, then drops
/// or replaces, grapheme by grapheme, what `policy` does not keep.
fn clean(input: &str, policy: Policy) -> Result<(String, usize), MailError> {
    check_input(input)?;
    let input = normalize_line_endings(input);
    let mut removed = 0;
    let mut cleaned = String::with_capacity(input.len());
    for cluster in input.graphemes(true) {
        clean_cluster(&mut cleaned, cluster, policy, &mut removed);
    }
    Ok((cleaned.nfc().collect(), removed))
}

fn check_input(input: &str) -> Result<(), MailError> {
    if input.contains('\0') {
        return Err(rejected("the text contains a NUL character"));
    }
    if input.chars().any(|ch| {
        matches!(get_general_category(ch), GeneralCategory::PrivateUse) || is_noncharacter(ch)
    }) {
        return Err(rejected(
            "the text contains a private-use or noncharacter code point",
        ));
    }
    Ok(())
}

fn normalize_line_endings(input: &str) -> Cow<'_, str> {
    if input.contains('\r') {
        Cow::Owned(input.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(input)
    }
}

fn clean_cluster(out: &mut String, cluster: &str, policy: Policy, removed: &mut usize) {
    if !emoji::is_candidate(cluster) {
        for ch in cluster.chars() {
            clean_char(out, ch, policy, removed);
        }
        return;
    }
    if emoji::is_valid(cluster) {
        out.push_str(cluster);
        return;
    }
    for ch in cluster.chars() {
        if emoji::is_emoji_format(ch) || matches!(get_general_category(ch), GeneralCategory::Format)
        {
            *removed += 1;
        } else {
            clean_char(out, ch, policy, removed);
        }
    }
}

fn clean_char(out: &mut String, ch: char, policy: Policy, removed: &mut usize) {
    match get_general_category(ch) {
        GeneralCategory::Control if matches!(ch, '\t' | '\n') => out.push(ch),
        GeneralCategory::Control
        | GeneralCategory::Format
        | GeneralCategory::Unassigned
        | GeneralCategory::PrivateUse => *removed += 1,
        GeneralCategory::LineSeparator | GeneralCategory::ParagraphSeparator => {
            out.push('\n');
            *removed += 1;
        }
        GeneralCategory::SpaceSeparator if policy == Policy::Incoming && ch != ' ' => {
            out.push(' ');
            *removed += 1;
        }
        _ => out.push(ch),
    }
}

fn check_limits(text: &str) -> Result<(), MailError> {
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(rejected("the text has more than 100000 characters"));
    }
    let mut graphemes = 0;
    for cluster in text.graphemes(true) {
        graphemes += 1;
        if graphemes > MAX_TEXT_GRAPHEMES {
            return Err(rejected("the text has more than 100000 graphemes"));
        }
        let marks = cluster
            .chars()
            .filter(|ch| {
                matches!(
                    get_general_category(*ch),
                    GeneralCategory::NonspacingMark
                        | GeneralCategory::SpacingMark
                        | GeneralCategory::EnclosingMark
                )
            })
            .count();
        if marks > MAX_MARKS_PER_GRAPHEME {
            return Err(rejected("a grapheme has more than 8 combining marks"));
        }
    }
    if text.split('\n').count() > MAX_TEXT_LINES {
        return Err(rejected("the text has more than 10000 lines"));
    }
    if text
        .split('\n')
        .any(|line| line.chars().count() > MAX_LINE_CHARS)
    {
        return Err(rejected("a line has more than 10000 characters"));
    }
    if !text.chars().any(is_visible) {
        return Err(rejected("the text has no visible content"));
    }
    Ok(())
}

fn is_visible(ch: char) -> bool {
    !ch.is_whitespace() && !is_default_ignorable(ch)
}

fn is_noncharacter(ch: char) -> bool {
    let code = ch as u32;
    (0xFDD0..=0xFDEF).contains(&code) || (code & 0xFFFE) == 0xFFFE
}

fn is_default_ignorable(ch: char) -> bool {
    let code = ch as u32;
    DEFAULT_IGNORABLE
        .binary_search_by(|&(low, high)| {
            if high < code {
                Ordering::Less
            } else if low > code {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .is_ok()
}

fn rejected(reason: &'static str) -> MailError {
    MailError::InvalidInput(Redacted::new(reason))
}

/// `Default_Ignorable_Code_Point` from `DerivedCoreProperties.txt`, merged.
const DEFAULT_IGNORABLE: [(u32, u32); 17] = [
    (0x00AD, 0x00AD),
    (0x034F, 0x034F),
    (0x061C, 0x061C),
    (0x115F, 0x1160),
    (0x17B4, 0x17B5),
    (0x180B, 0x180F),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x206F),
    (0x3164, 0x3164),
    (0xFE00, 0xFE0F),
    (0xFEFF, 0xFEFF),
    (0xFFA0, 0xFFA0),
    (0xFFF0, 0xFFF8),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0000, 0xE0FFF),
];
