//! Corrects the engine's text against the project's closed lexicon. A run of one to three words
//! becomes a term when its Spanish sound is close enough to the term's sound. The rules and the
//! threshold are the ones `research/voz-spike/bench/corrector.py` measured on the 40 phrases.

#[cfg(test)]
mod tests;

use std::collections::HashSet;
use std::ops::Range;
use std::sync::OnceLock;

use crate::corrector_data::{COMMON_WORDS, LEXICON};

/// Largest relative distance between two sounds for a run to change to a term.
const THRESHOLD: f64 = 0.25;
const MAX_RUN: usize = 3;

/// Whether the engine's text goes through the corrector. It is on unless the host starts with
/// `--asr-no-corrector`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Corrector {
    #[default]
    On,
    Off,
}

impl Corrector {
    pub(crate) fn apply(self, text: &str) -> String {
        match self {
            Self::On => correct(text),
            Self::Off => text.to_owned(),
        }
    }
}

/// Rewrites the words that sound like a term. Punctuation and spacing stay as they are.
pub(crate) fn correct(text: &str) -> String {
    correct_with(text, THRESHOLD)
}

fn correct_with(text: &str, threshold: f64) -> String {
    let words = word_ranges(text);
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut i = 0;
    while i < words.len() {
        match best_run(text, &words[i..], threshold) {
            None => i += 1,
            Some(run) => {
                let first = words[i].start;
                out.push_str(&text[copied..first]);
                out.push_str(&written_like(run.term, &text[first..]));
                copied = words[i + run.len - 1].end;
                i += run.len;
            }
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// Byte ranges of the words in `text`: runs of letters and digits.
fn word_ranges(text: &str) -> Vec<Range<usize>> {
    let mut words = Vec::new();
    let mut start = None;
    for (at, letter) in text.char_indices() {
        match (letter.is_alphanumeric(), start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                words.push(from..at);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        words.push(from..text.len());
    }
    words
}

/// Words that change to one term: how many, and how far their sound is from the term's.
struct Run {
    len: usize,
    term: &'static str,
    distance: f64,
}

/// The run that starts `words` and sounds closest to a term. On a tie the longer run wins. A run
/// never holds a common word unless it is one word that maps to itself.
fn best_run(text: &str, words: &[Range<usize>], threshold: f64) -> Option<Run> {
    let mut best: Option<Run> = None;
    for len in 1..=MAX_RUN.min(words.len()) {
        let run: Vec<&str> = words[..len]
            .iter()
            .map(|word| &text[word.clone()])
            .collect();
        if len > 1 && run.iter().any(|word| is_common(word)) {
            break;
        }
        let Some((term, distance)) = closest_term(&run, threshold) else {
            continue;
        };
        if len == 1 && is_common(run[0]) && distance > 0.0 {
            continue;
        }
        if best.as_ref().is_none_or(|kept| distance <= kept.distance) {
            best = Some(Run {
                len,
                term,
                distance,
            });
        }
    }
    best
}

/// The lexicon term whose sound is closest to the sound of `words`, if within `threshold`. On a tie
/// the term listed first wins.
fn closest_term(words: &[&str], threshold: f64) -> Option<(&'static str, f64)> {
    let sound: String = words.iter().map(|word| sound_of(word)).collect();
    let mut best: Option<(&'static str, f64)> = None;
    for (term, term_sound) in lexicon_sounds() {
        let longest = sound.chars().count().max(term_sound.chars().count());
        let distance = edit_distance(&sound, term_sound) as f64 / longest as f64;
        if distance <= threshold && best.is_none_or(|(_, closest)| distance < closest) {
            best = Some((term, distance));
        }
    }
    best
}

fn lexicon_sounds() -> &'static [(&'static str, String)] {
    static SOUNDS: OnceLock<Vec<(&'static str, String)>> = OnceLock::new();
    SOUNDS.get_or_init(|| LEXICON.iter().map(|term| (*term, sound_of(term))).collect())
}

/// The term written with the capital the heard word had, if any.
fn written_like(term: &str, heard: &str) -> String {
    if !heard.starts_with(char::is_uppercase) {
        return term.to_owned();
    }
    let mut letters = term.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => String::new(),
    }
}

/// The Spanish sound of a word in simplified form: lowercase, no accents, and the rules in order.
fn sound_of(word: &str) -> String {
    RULES
        .iter()
        .fold(plain(word), |sound, rule| rule.apply(&sound))
}

/// Rewrites `from` to `to` when `followed_by` is empty or the next letter is one of its letters.
struct Rule {
    from: &'static str,
    followed_by: &'static str,
    to: &'static str,
}

/// The order matters: each rule reads the sound the rules before it left.
const RULES: &[Rule] = &[
    Rule {
        from: "h",
        followed_by: "",
        to: "",
    },
    Rule {
        from: "ll",
        followed_by: "",
        to: "y",
    },
    Rule {
        from: "qu",
        followed_by: "",
        to: "k",
    },
    Rule {
        from: "gu",
        followed_by: "ei",
        to: "g",
    },
    Rule {
        from: "c",
        followed_by: "ei",
        to: "s",
    },
    Rule {
        from: "z",
        followed_by: "ei",
        to: "s",
    },
    Rule {
        from: "c",
        followed_by: "",
        to: "k",
    },
    Rule {
        from: "z",
        followed_by: "",
        to: "s",
    },
    Rule {
        from: "x",
        followed_by: "",
        to: "s",
    },
    Rule {
        from: "v",
        followed_by: "",
        to: "b",
    },
    Rule {
        from: "q",
        followed_by: "",
        to: "k",
    },
    Rule {
        from: "g",
        followed_by: "ei",
        to: "j",
    },
];

impl Rule {
    fn apply(&self, sound: &str) -> String {
        let mut out = String::with_capacity(sound.len());
        let mut rest = sound;
        while !rest.is_empty() {
            if self.matches(rest) {
                out.push_str(self.to);
                rest = &rest[self.from.len()..];
            } else {
                let width = rest.chars().next().map_or(rest.len(), char::len_utf8);
                out.push_str(&rest[..width]);
                rest = &rest[width..];
            }
        }
        out
    }

    fn matches(&self, rest: &str) -> bool {
        let Some(after) = rest.strip_prefix(self.from) else {
            return false;
        };
        self.followed_by.is_empty()
            || after.starts_with(|letter: char| self.followed_by.contains(letter))
    }
}

/// Lowercase without accents: «Aprobación» becomes «aprobacion».
fn plain(word: &str) -> String {
    word.to_lowercase().chars().filter_map(unaccented).collect()
}

fn unaccented(letter: char) -> Option<char> {
    Some(match letter {
        'á' | 'à' | 'â' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        '\u{300}'..='\u{36f}' => return None,
        other => other,
    })
}

fn is_common(word: &str) -> bool {
    common_words().contains(&plain(word))
}

fn common_words() -> &'static HashSet<String> {
    static COMMON: OnceLock<HashSet<String>> = OnceLock::new();
    COMMON.get_or_init(|| COMMON_WORDS.split_whitespace().map(plain).collect())
}

/// Levenshtein distance, counted in letters.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, letter) in a.chars().enumerate() {
        let mut row = vec![i + 1];
        for (j, other) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(letter != *other);
            let next = (previous[j + 1] + 1).min(row[j] + 1).min(substitution);
            row.push(next);
        }
        previous = row;
    }
    previous[b.len()]
}
