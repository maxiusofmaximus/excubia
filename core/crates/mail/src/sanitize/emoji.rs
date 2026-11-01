//! The emoji exception to "every format character goes": a grapheme cluster
//! that spells a UTS #51 sequence keeps its ZWJ, VS16, keycap and tags.
//!
//! The check is grammar, not a list of known emoji: a base must be
//! `Extended_Pictographic` (or a regional-indicator flag), one modifier and
//! VS16 may follow a base, and a ZWJ may only join one emoji element to
//! another. The base table is pinned to Unicode 16.0 so a new Unicode release
//! cannot silently change what the mail path accepts.

use std::cmp::Ordering;

use unicode_general_category::{get_general_category, GeneralCategory};

/// The Unicode version [`EXTENDED_PICTOGRAPHIC`] was copied from.
pub const UNICODE_VERSION: &str = "16.0";

const ZWJ: char = '\u{200D}';
const VS16: char = '\u{FE0F}';
const KEYCAP: char = '\u{20E3}';
const TAG_FLAG: char = '\u{1F3F4}';
const TAG_END: char = '\u{E007F}';

/// `Extended_Pictographic` from `emoji-data.txt`, merged and sorted.
const EXTENDED_PICTOGRAPHIC: [(u32, u32); 78] = [
    (0x00A9, 0x00A9),
    (0x00AE, 0x00AE),
    (0x203C, 0x203C),
    (0x2049, 0x2049),
    (0x2122, 0x2122),
    (0x2139, 0x2139),
    (0x2194, 0x2199),
    (0x21A9, 0x21AA),
    (0x231A, 0x231B),
    (0x2328, 0x2328),
    (0x2388, 0x2388),
    (0x23CF, 0x23CF),
    (0x23E9, 0x23F3),
    (0x23F8, 0x23FA),
    (0x24C2, 0x24C2),
    (0x25AA, 0x25AB),
    (0x25B6, 0x25B6),
    (0x25C0, 0x25C0),
    (0x25FB, 0x25FE),
    (0x2600, 0x2605),
    (0x2607, 0x2612),
    (0x2614, 0x2685),
    (0x2690, 0x2705),
    (0x2708, 0x2712),
    (0x2714, 0x2714),
    (0x2716, 0x2716),
    (0x271D, 0x271D),
    (0x2721, 0x2721),
    (0x2728, 0x2728),
    (0x2733, 0x2734),
    (0x2744, 0x2744),
    (0x2747, 0x2747),
    (0x274C, 0x274C),
    (0x274E, 0x274E),
    (0x2753, 0x2755),
    (0x2757, 0x2757),
    (0x2763, 0x2767),
    (0x2795, 0x2797),
    (0x27A1, 0x27A1),
    (0x27B0, 0x27B0),
    (0x27BF, 0x27BF),
    (0x2934, 0x2935),
    (0x2B05, 0x2B07),
    (0x2B1B, 0x2B1C),
    (0x2B50, 0x2B50),
    (0x2B55, 0x2B55),
    (0x3030, 0x3030),
    (0x303D, 0x303D),
    (0x3297, 0x3297),
    (0x3299, 0x3299),
    (0x1F000, 0x1F0FF),
    (0x1F10D, 0x1F10F),
    (0x1F12F, 0x1F12F),
    (0x1F16C, 0x1F171),
    (0x1F17E, 0x1F17F),
    (0x1F18E, 0x1F18E),
    (0x1F191, 0x1F19A),
    (0x1F1AD, 0x1F1E5),
    (0x1F201, 0x1F20F),
    (0x1F21A, 0x1F21A),
    (0x1F22F, 0x1F22F),
    (0x1F232, 0x1F23A),
    (0x1F23C, 0x1F23F),
    (0x1F249, 0x1F3FA),
    (0x1F400, 0x1F53D),
    (0x1F546, 0x1F64F),
    (0x1F680, 0x1F6FF),
    (0x1F774, 0x1F77F),
    (0x1F7D5, 0x1F7FF),
    (0x1F80C, 0x1F80F),
    (0x1F848, 0x1F84F),
    (0x1F85A, 0x1F85F),
    (0x1F888, 0x1F88F),
    (0x1F8AE, 0x1F8FF),
    (0x1F90C, 0x1F93A),
    (0x1F93C, 0x1F945),
    (0x1F947, 0x1FAFF),
    (0x1FC00, 0x1FFFD),
];

/// Whether the cluster holds a character the grammar may excuse.
pub(super) fn is_candidate(cluster: &str) -> bool {
    cluster.chars().any(is_emoji_format)
}

/// A joiner, a variation selector or a tag: removed unless the whole cluster
/// is a valid emoji sequence.
pub(super) fn is_emoji_format(ch: char) -> bool {
    ch == ZWJ || is_variation_selector(ch) || is_tag(ch)
}

/// Whether every character of the cluster belongs to one emoji sequence.
pub(super) fn is_valid(cluster: &str) -> bool {
    let chars: Vec<char> = cluster.chars().collect();
    !chars.is_empty()
        && !chars
            .iter()
            .any(|&ch| matches!(get_general_category(ch), GeneralCategory::Unassigned))
        && valid_sequence(&chars)
}

fn valid_sequence(chars: &[char]) -> bool {
    let Some(&first) = chars.first() else {
        return false;
    };
    if first == TAG_FLAG {
        return is_tag_flag(chars);
    }
    if is_regional_indicator(first) {
        return is_regional_indicator_flag(chars);
    }
    if is_keycap_base(first) {
        return is_keycap(chars);
    }
    is_zwj_sequence(chars)
}

/// `U+1F3F4` followed by one or more `[a-z]` tags and `U+E007F`, and nothing
/// else: the only tag sequence the mail path carries.
fn is_tag_flag(chars: &[char]) -> bool {
    let Some((&first, rest)) = chars.split_first() else {
        return false;
    };
    let Some((&last, body)) = rest.split_last() else {
        return false;
    };
    first == TAG_FLAG
        && last == TAG_END
        && !body.is_empty()
        && body
            .iter()
            .all(|&ch| ('\u{E0061}'..='\u{E007A}').contains(&ch))
}

fn is_regional_indicator_flag(chars: &[char]) -> bool {
    chars.len() == 2 && chars.iter().copied().all(is_regional_indicator)
}

fn is_keycap(chars: &[char]) -> bool {
    let Some(&base) = chars.first() else {
        return false;
    };
    if !is_keycap_base(base) || chars.last() != Some(&KEYCAP) {
        return false;
    }
    match chars.len() {
        2 => true,
        3 => chars[1] == VS16,
        _ => false,
    }
}

/// An emoji element, then zero or more `ZWJ` joined elements.
fn is_zwj_sequence(chars: &[char]) -> bool {
    let mut index = 0;
    loop {
        if !chars
            .get(index)
            .is_some_and(|ch| is_extended_pictographic(*ch))
        {
            return false;
        }
        index += 1;
        if chars.get(index).is_some_and(|ch| is_modifier(*ch)) {
            index += 1;
        }
        if chars.get(index) == Some(&VS16) {
            index += 1;
        }
        match chars.get(index) {
            None => return true,
            Some(&ZWJ) => index += 1,
            Some(_) => return false,
        }
    }
}

fn is_extended_pictographic(ch: char) -> bool {
    let code = ch as u32;
    EXTENDED_PICTOGRAPHIC
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

fn is_variation_selector(ch: char) -> bool {
    matches!(ch as u32, 0xFE00..=0xFE0F | 0xE0100..=0xE01EF)
}

fn is_tag(ch: char) -> bool {
    matches!(ch as u32, 0xE0020..=0xE007F)
}

fn is_regional_indicator(ch: char) -> bool {
    matches!(ch as u32, 0x1F1E6..=0x1F1FF)
}

fn is_modifier(ch: char) -> bool {
    matches!(ch as u32, 0x1F3FB..=0x1F3FF)
}

fn is_keycap_base(ch: char) -> bool {
    matches!(ch, '0'..='9' | '#' | '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_base_table_is_the_pinned_unicode_version() {
        assert_eq!(UNICODE_VERSION, "16.0");
        assert!(is_extended_pictographic('\u{00A9}'));
        assert!(is_extended_pictographic('\u{1F600}'));
        assert!(is_extended_pictographic('\u{1F3F4}'));
        assert!(!is_extended_pictographic('\u{1F1E6}'));
        assert!(!is_extended_pictographic('A'));
    }

    #[test]
    fn a_zwj_must_join_two_emoji_elements() {
        assert!(is_valid("\u{1F468}\u{200D}\u{1F469}"));
        assert!(!is_valid("a\u{200D}\u{1F469}"));
        assert!(!is_valid("\u{1F468}\u{200D}a"));
    }
}
