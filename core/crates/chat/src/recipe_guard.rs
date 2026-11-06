//! What a recipe may never keep (F5). A secret is refused outright, even when the
//! person confirms it. An order that overrides the person or acts without asking
//! is a risk: kept only when the person confirms it. Each reason is a plain
//! sentence the app can show as it is.

use std::sync::LazyLock;

use regex::Regex;

use crate::error::ChatError;
use crate::model_recipe::{check_text, RecipeText};

const KEY_TOKEN: &str = "contiene una cadena larga de letras y números (posible clave)";

struct Rule {
    pattern: Regex,
    reason: &'static str,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Scan {
    pub secrets: Vec<&'static str>,
    pub risks: Vec<&'static str>,
}

static SECRETS: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    rules(&[
        (r"sk-[A-Za-z0-9_\-]{20,}", "parece una clave de API (sk-…)"),
        (
            r"gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}",
            "parece un token de GitHub",
        ),
        (r"AKIA[0-9A-Z]{16}", "parece una clave de AWS (AKIA…)"),
        (
            r"\b[0-9a-fA-F]{32,}\b",
            "parece una cadena hexadecimal larga (posible clave)",
        ),
        (
            r"(?i)(password|passwd|contraseña|contrasena|clave)\s*[:=]\s*\S+",
            "parece una contraseña escrita como «clave: valor»",
        ),
        (
            r"-----BEGIN [A-Z0-9 ]*KEY-----",
            "contiene una clave privada",
        ),
        (
            r"[A-Za-z][A-Za-z0-9+.\-]*://[^\s/:@]+:[^\s/@]+@",
            "lleva usuario y contraseña dentro de una dirección web",
        ),
    ])
});

/// Matched on the folded text: lowercase, without the accents of Spanish.
static RISKS: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    rules(&[
        (
            r"ignora (todas )?(las )?instrucciones|ignore (all )?(the )?(previous|prior|above) instructions",
            "intenta anular las instrucciones anteriores",
        ),
        (
            r"borra todo|borrar todo|delete everything",
            "ordena borrar todo",
        ),
        (
            r"sin (pedir |esperar )?aprobacion|without (asking for )?approval",
            "pide actuar sin aprobación",
        ),
    ])
});

fn rules(list: &[(&str, &'static str)]) -> Vec<Rule> {
    list.iter()
        .map(|(pattern, reason)| Rule {
            pattern: Regex::new(pattern).expect("static recipe pattern"),
            reason,
        })
        .collect()
}

/// Reads the title and the body together: a secret or an order in either is found.
pub fn scan(title: &str, body: &str) -> Scan {
    let text = format!("{title}\n{body}");
    let folded = fold(&text);
    let mut secrets: Vec<&'static str> = hits(&SECRETS, &text).collect();
    if has_key_token(&text) {
        secrets.push(KEY_TOKEN);
    }
    Scan {
        secrets,
        risks: hits(&RISKS, &folded).collect(),
    }
}

/// The gate of a save: the text rules first, then the scan. A secret is refused
/// even when confirmed; a risk needs `confirm_risks`.
pub fn admit(text: &RecipeText, confirm_risks: bool) -> Result<RecipeText, ChatError> {
    let text = check_text(text)?;
    let found = scan(&text.title, &text.body);
    if !found.secrets.is_empty() {
        return Err(ChatError::RecipeSecret(found.secrets));
    }
    if !found.risks.is_empty() && !confirm_risks {
        return Err(ChatError::RecipeRisk(found.risks));
    }
    Ok(text)
}

fn hits<'a>(rules: &'a [Rule], text: &'a str) -> impl Iterator<Item = &'static str> + 'a {
    rules
        .iter()
        .filter(move |rule| rule.pattern.is_match(text))
        .map(|rule| rule.reason)
}

/// A run of 40 or more key characters holding both letters and digits: a token,
/// not a word.
fn has_key_token(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '_' | '-' | '=')))
        .any(|token| {
            token.len() >= 40
                && token.chars().any(|c| c.is_ascii_digit())
                && token.chars().any(|c| c.is_ascii_alphabetic())
        })
}

fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

#[cfg(test)]
#[path = "recipe_guard_tests.rs"]
mod tests;
