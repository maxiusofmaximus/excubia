//! The style check of a draft (ESTILO0, RFC estilo-aprendido §4.2): the host refuses a
//! subject or body with a `;` outside URLs and code blocks, or with a word of the
//! operator's list `estilo/tecnicas.txt`. Plain string work, no network. A refused draft
//! is not created, and the refusal names only the fragment (60 characters at most) and
//! the change to make.

use std::fs::{create_dir_all, read_to_string, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::Path;

use excubia_chat::{ToolError, ToolReply};
use excubia_mail::{MailError, Redacted};
use excubia_seat::DraftFields;
use serde_json::json;

/// The operator's word list, under the data dir. Created with the seed if missing.
pub const WORD_LIST_FILE: &str = "estilo/tecnicas.txt";
/// The operator's writing rules, read by `style.get` (RFC estilo-aprendido §3, F0).
pub const PROFILE_FILE: &str = "estilo/estilo.md";
const PROFILE_SEED: &str = "# Cómo escribe el Master (reglas aceptadas). Edítalo a mano.\n\
1. Nunca uses punto y coma (;). Usa punto o coma.\n\
2. Lenguaje sencillo: nada de palabras técnicas (commit, CI, refactorización, repositorio...). Explica lo que se hizo como se lo contarías a alguien que no es programador.\n\
3. Frases cortas y directas.\n\
4. Respeta el saludo, la despedida y la firma que el Master te dé o que estén en sus correos.\n";
const MAX_PROFILE: usize = 4096;
const SEED: &str = "# Palabras técnicas que un borrador no admite. Una por línea; # comenta.\n\
# Sin distinguir mayúsculas ni tildes; solo palabra completa.\n\
commit\nCI\nhook\nRLS\nCVE\nrefactorización\nrefactorizar\ncargue masivo\ndevcontainer\n\
repositorio\nrama main\n";
const MAX_NOTES: usize = 5;
const MAX_FRAGMENT: usize = 60;

/// Writes the seed list when the file does not exist. An existing file is never overwritten.
pub fn seed_word_list(data_dir: &Path) -> Result<(), MailError> {
    seed_file(data_dir, WORD_LIST_FILE, SEED)
}

/// Writes the style rules when the file does not exist. An existing file is never overwritten.
pub fn seed_profile(data_dir: &Path) -> Result<(), MailError> {
    seed_file(data_dir, PROFILE_FILE, PROFILE_SEED)
}

fn seed_file(data_dir: &Path, relative: &str, seed: &str) -> Result<(), MailError> {
    let path = data_dir.join(relative);
    if let Some(parent) = path.parent() {
        create_dir_all(parent).map_err(store)?;
    }
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => file.write_all(seed.as_bytes()).map_err(store),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(store(error)),
    }
}

/// `style.get`: the text of the style rules, exactly as the file holds it. A file over
/// 4 KiB is refused with `style_too_large`, never cut.
pub fn read_profile(path: &Path) -> Result<ToolReply, ToolError> {
    let unreadable = |_: std::io::Error| {
        ToolError::unavailable("the style profile cannot be read: check estilo/estilo.md")
    };
    let text = read_to_string(path).map_err(unreadable)?;
    if text.len() > MAX_PROFILE {
        return Err(ToolError::invalid(
            "style_too_large",
            "estilo.md",
            "estilo/estilo.md pasa de 4 KiB: acórtalo o divídelo en el archivo",
        ));
    }
    Ok(ToolReply {
        result: json!({ "ok": true, "text": text }),
    })
}

fn store(error: std::io::Error) -> MailError {
    MailError::Store(Redacted::new(error.to_string()))
}

/// Refuses the draft with `style_rejected` when its subject or body breaks the style rules.
pub fn check_draft(word_list: &Path, fields: &DraftFields) -> Result<(), ToolError> {
    let words = read_words(word_list).map_err(|_| {
        ToolError::unavailable("the style word list cannot be read: check estilo/tecnicas.txt")
    })?;
    let subject = findings("subject", &fields.subject, &words);
    let body = findings("body", &fields.body, &words);
    if subject.is_empty() && body.is_empty() {
        return Ok(());
    }
    let field = if subject.is_empty() {
        "body"
    } else {
        "subject"
    };
    let notes: Vec<String> = subject.into_iter().chain(body).take(MAX_NOTES).collect();
    Err(ToolError::invalid(
        "style_rejected",
        field,
        format!("El borrador no se creó. Corrige: {}", notes.join(" ")),
    ))
}

/// One message per fragment that breaks a rule, in the order the text has them.
fn findings(field: &str, text: &str, words: &[Vec<String>]) -> Vec<String> {
    let mut notes = Vec::new();
    for fragment in semicolons(text) {
        notes.push(format!(
            "{field}: «{fragment}» tiene punto y coma; usa «.» o «,»."
        ));
    }
    for fragment in listed_words(text, words) {
        notes.push(format!(
            "{field}: «{fragment}» es jerga técnica; dilo con palabras sencillas."
        ));
    }
    notes
}

/// The chunks with a `;` outside URLs, code blocks and HTML entities such as `&nbsp;`.
fn semicolons(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    // Splitting on the fence puts the code blocks at the odd indexes; keep the prose.
    for prose in text.split("```").step_by(2) {
        for chunk in prose.split_whitespace() {
            if chunk.contains("http://") || chunk.contains("https://") {
                continue;
            }
            let stray = chunk
                .char_indices()
                .any(|(at, c)| c == ';' && !is_entity(&chunk[..at]));
            if stray {
                found.push(clip(chunk));
            }
        }
    }
    found
}

/// True when `before` ends in `&name`, so the `;` that follows closes an HTML entity.
fn is_entity(before: &str) -> bool {
    match before.rfind('&') {
        Some(at) => {
            let name = &before[at + 1..];
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '#')
        }
        None => false,
    }
}

/// The text of each listed word or phrase found as whole words, without case or accents.
fn listed_words(text: &str, words: &[Vec<String>]) -> Vec<String> {
    let found = tokens(text);
    let mut matches = Vec::new();
    for entry in words {
        let n = entry.len();
        for start in 0..(found.len() + 1).saturating_sub(n) {
            let same = found[start..start + n]
                .iter()
                .map(|token| &token.0)
                .eq(entry.iter());
            if same {
                let (from, to) = (found[start].1, found[start + n - 1].2);
                matches.push(clip(&text[from..to]));
            }
        }
    }
    matches
}

/// The word list: one entry per usable line, each as its folded words.
fn read_words(path: &Path) -> std::io::Result<Vec<Vec<String>>> {
    let text = read_to_string(path)?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| tokens(line).into_iter().map(|token| token.0).collect())
        .filter(|entry: &Vec<String>| !entry.is_empty())
        .collect())
}

/// The words of `text` as (folded word, start, end) byte offsets into `text`.
pub(crate) fn tokens(text: &str) -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let mut current: Option<(String, usize)> = None;
    for (at, c) in text.char_indices() {
        if c.is_alphanumeric() {
            match current.as_mut() {
                Some((word, _)) => word.push(fold(c)),
                None => current = Some((fold(c).to_string(), at)),
            }
        } else if let Some((word, start)) = current.take() {
            out.push((word, start, at));
        }
    }
    if let Some((word, start)) = current {
        out.push((word, start, text.len()));
    }
    out
}

/// Lower case, and the accents of Spanish vowels removed.
fn fold(c: char) -> char {
    match c.to_lowercase().next().unwrap_or(c) {
        'á' | 'à' | 'ä' | 'â' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        other => other,
    }
}

fn clip(text: &str) -> String {
    text.chars().take(MAX_FRAGMENT).collect()
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    use excubia_test_dir::TestDir;

    #[test]
    fn the_profile_is_seeded_once_and_never_overwritten() {
        let dir = TestDir::new("style-profile-seed");
        seed_profile(&dir).expect("first seed");
        let path = dir.join(PROFILE_FILE);
        assert_eq!(
            std::fs::read_to_string(&path).expect("seeded"),
            PROFILE_SEED
        );

        std::fs::write(&path, "Mi regla propia.\n").expect("operator edit");
        seed_profile(&dir).expect("second seed");
        assert_eq!(
            std::fs::read_to_string(&path).expect("kept"),
            "Mi regla propia.\n"
        );
    }

    #[test]
    fn style_get_returns_the_exact_text() {
        let dir = TestDir::new("style-profile-get");
        seed_profile(&dir).expect("seed");
        let Ok(reply) = read_profile(&dir.join(PROFILE_FILE)) else {
            panic!("the seeded profile reads");
        };
        assert_eq!(reply.result["text"], PROFILE_SEED);
    }

    #[test]
    fn a_profile_over_4_kib_is_refused_and_exactly_4_kib_is_not() {
        let dir = TestDir::new("style-profile-large");
        let path = dir.join(PROFILE_FILE);
        std::fs::create_dir_all(dir.join("estilo")).expect("folder");

        std::fs::write(&path, "a".repeat(MAX_PROFILE + 1)).expect("too large");
        let Err(refused) = read_profile(&path) else {
            panic!("over 4 KiB must be refused");
        };
        assert_eq!(refused.code, "style_too_large");

        std::fs::write(&path, "a".repeat(MAX_PROFILE)).expect("exactly 4 KiB");
        assert!(read_profile(&path).is_ok());
    }

    #[test]
    fn a_missing_profile_is_seat_unavailable() {
        let dir = TestDir::new("style-profile-missing");
        let Err(refused) = read_profile(&dir.join(PROFILE_FILE)) else {
            panic!("a missing profile is refused");
        };
        assert_eq!(refused.code, "seat_unavailable");
    }
}
