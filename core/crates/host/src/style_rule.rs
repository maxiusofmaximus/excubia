//! `style.propose_rule` (RFC estilo-aprendido §2.2 and §3, F2, ESTILO3): Alfred proposes one
//! line for the operator's writing rules. The host checks the line and writes it into
//! `estilo/estilo.md` at once, without a card. A copy of the old file goes to
//! `estilo/historial/` first. A line the profile already says is not written twice.
//! `apply_approved_rules` still applies the old cards that were approved before this change.

use std::fs::{create_dir_all, read_to_string};
use std::path::{Path, PathBuf};
use std::sync::PoisonError;

use chrono::Local;
use excubia_chat::{SeatActor, ToolError, ToolReply};
use excubia_persist::write_atomic;
use excubia_seat::StyleProposal;
use serde_json::json;

use crate::mail_style::tokens;
use crate::mail_tools::MailTools;
use crate::style_learn::safe;

/// The action of the old cards. Only `apply_approved_rules` reads it now.
const ACTION: &str = "style.propose_rule";
/// The profile's size limit, in bytes (RFC §3).
const MAX_PROFILE: usize = 4096;
/// The most numbered rules a profile may hold (RFC §3).
const MAX_RULES: usize = 40;
/// Where the copies of the old profile go, under the profile's folder.
const HISTORY_DIR: &str = "historial";

impl MailTools {
    /// Checks the line and writes it into `estilo.md` at once, as the next numbered rule.
    /// A line the profile already says is not written again.
    pub async fn style_propose(
        &self,
        _actor: &SeatActor,
        proposal: &StyleProposal,
    ) -> Result<ToolReply, ToolError> {
        self.apply_approved_rules();
        check_content(&proposal.rule)?;
        // One at a time: two lines asked together must not both read the same profile.
        let _turn = self
            .style_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if already_saved(&read_text(&self.profile)?, &proposal.rule) {
            return Ok(ToolReply {
                result: json!({ "ok": true, "status": "already_saved" }),
            });
        }
        save(&self.profile, &proposal.rule, &proposal.source)?;
        Ok(ToolReply {
            result: json!({ "ok": true, "status": "saved" }),
        })
    }

    /// Writes the approved lines into `estilo.md`, in the order they were asked, and consumes
    /// each one, so a line is never written twice. A line that no longer fits is consumed
    /// without a write. A line that cannot be written stays approved, and the ones after it
    /// wait, so the order holds. Logs what it does and never fails its caller.
    pub fn apply_approved_rules(&self) {
        // Two callers at once would both write the same line; one at a time keeps it once.
        let _turn = self
            .style_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let approved = match self.chat.approved_for_tool(ACTION) {
            Ok(approved) => approved,
            Err(error) => {
                eprintln!("host: could not read the approved style rules: {error}");
                return;
            }
        };
        for approval in approved {
            let payload = approval.payload_redacted.unwrap_or_default();
            let rule = payload["rule"].as_str().unwrap_or_default();
            let source = payload["source"].as_str().unwrap_or_default();
            match save(&self.profile, rule, source) {
                Ok(()) => {}
                Err(error) if error.code == "style_too_large" || error.code == "style_too_many" => {
                    eprintln!(
                        "host: an approved style rule does not fit ({}): not written",
                        error.code
                    );
                }
                Err(error) => {
                    eprintln!(
                        "host: an approved style rule waits, not written ({})",
                        error.code
                    );
                    return;
                }
            }
            if let Err(error) = self
                .chat
                .consume_for_host(&approval.id, &approval.action_digest)
            {
                eprintln!("host: an approved style rule was not consumed: {error}");
                return;
            }
        }
    }
}

/// Refuses a line that carries an address, a link, a phone number, or a word about tools,
/// approvals or sending. Those never belong in the writing rules (RFC §3).
pub fn check_content(rule: &str) -> Result<(), ToolError> {
    let folded = fold_words(rule);
    let lower = rule.to_lowercase();
    let mentions = ["envi", "send", "aprob", "approval", "tool"]
        .iter()
        .any(|word| folded.contains(word))
        || lower.contains("mail.");
    if !safe(rule) || mentions {
        return Err(ToolError::invalid(
            "invalid_arguments",
            "rule",
            "la regla no puede llevar correos, enlaces, teléfonos ni palabras de herramientas, aprobaciones o envíos",
        ));
    }
    Ok(())
}

/// The profile with `rule` added as its next numbered line. Refused, never cut:
/// `style_too_many` at 40 rules, `style_too_large` past 4 KiB.
pub fn next_profile(
    current: &str,
    rule: &str,
    source: &str,
    date: &str,
) -> Result<String, ToolError> {
    let numbers: Vec<u32> = current.lines().filter_map(rule_number).collect();
    if numbers.len() >= MAX_RULES {
        return Err(ToolError::invalid(
            "style_too_many",
            "estilo.md",
            "estilo/estilo.md ya tiene 40 reglas: quita una a mano",
        ));
    }
    let next = numbers.iter().max().map_or(1, |last| last + 1);
    let mut text = current.to_owned();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!("{next}. {rule} (origen: {source}, {date})\n"));
    if text.len() > MAX_PROFILE {
        return Err(ToolError::invalid(
            "style_too_large",
            "estilo.md",
            "estilo/estilo.md pasaría de 4 KiB: acórtalo a mano",
        ));
    }
    Ok(text)
}

/// True when a numbered line of the profile already says `rule`: the same words, in any case
/// or accent, with or without its `(origen: …, fecha)` suffix.
fn already_saved(current: &str, rule: &str) -> bool {
    let wanted = fold_words(rule);
    current
        .lines()
        .filter(|line| rule_number(line).is_some())
        .any(|line| {
            let text = line.split_once(". ").map_or("", |(_, text)| text);
            let text = text
                .rsplit_once(" (origen: ")
                .map_or(text, |(text, _)| text);
            fold_words(text) == wanted
        })
}

/// The words of `text`, lower case and without accents, joined by one space.
fn fold_words(text: &str) -> String {
    tokens(text)
        .into_iter()
        .map(|token| token.0)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The number at the start of a rule line, such as `3. ...`. Other lines have none.
fn rule_number(line: &str) -> Option<u32> {
    let (digits, _) = line.split_once(". ")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Copies the old profile to `historial/`, then writes the new one. Both writes are atomic.
fn save(profile: &Path, rule: &str, source: &str) -> Result<(), ToolError> {
    let now = Local::now();
    let current = read_text(profile)?;
    let next = next_profile(&current, rule, source, &now.format("%Y-%m-%d").to_string())?;
    let history = profile.parent().unwrap_or(Path::new(".")).join(HISTORY_DIR);
    create_dir_all(&history).map_err(|_| unwritable())?;
    let stamp = now.format("%Y-%m-%d-%H%M%S").to_string();
    write_atomic(&free_name(&history, &stamp), current.as_bytes()).map_err(|_| unwritable())?;
    write_atomic(profile, next.as_bytes()).map_err(|_| unwritable())
}

/// `historial/AAAA-MM-DD-HHMMSS.md`, with `-2`, `-3`... when that second already has a copy.
fn free_name(dir: &Path, stamp: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stamp}.md"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stamp}-{n}.md"));
        n += 1;
    }
    candidate
}

fn read_text(path: &Path) -> Result<String, ToolError> {
    read_to_string(path).map_err(|_| {
        ToolError::unavailable("the style profile cannot be read: check estilo/estilo.md")
    })
}

fn unwritable() -> ToolError {
    ToolError::unavailable("estilo/estilo.md cannot be written; it did not change")
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str =
        "# Cómo escribe el Master.\n1. Nunca uses punto y coma.\n3. Frases cortas.\n";

    #[test]
    fn the_next_line_takes_the_next_number_and_its_origin_and_date() {
        let text =
            next_profile(HEADER, "Trata de usted.", "correccion", "2026-11-14").expect("fits");
        assert!(text.ends_with("4. Trata de usted. (origen: correccion, 2026-11-14)\n"));
        assert_eq!(text.matches("(origen:").count(), 1);
    }

    #[test]
    fn forty_rules_are_the_most_and_the_41st_is_refused() {
        let full: String = (1..=40).map(|n| format!("{n}. Regla.\n")).collect();
        let refused = next_profile(&full, "Otra.", "manual", "2026-11-14");
        assert_eq!(refused.expect_err("too many").code, "style_too_many");
        // Without its last line the profile holds 39 rules, so one more still fits.
        assert!(next_profile(
            &full[..full.len() - "40. Regla.\n".len()],
            "Otra.",
            "manual",
            "2026-11-14"
        )
        .is_ok());
    }

    #[test]
    fn a_profile_past_4_kib_is_refused_and_exactly_4_kib_is_not() {
        let base = "a".repeat(MAX_PROFILE - 60);
        let refused = next_profile(&base, &"b".repeat(300), "manual", "2026-11-14");
        assert_eq!(refused.expect_err("too large").code, "style_too_large");
    }

    #[test]
    fn a_rule_may_not_carry_an_address_a_link_a_phone_or_a_tool_word() {
        for rule in [
            "Escribe a juan@example.com.",
            "Mira http://x.es/a.",
            "Llama al 600 123 456.",
            "Nunca envía correos sin revisión.",
            "Usa la herramienta mail.move.",
            "Pide una aprobación.",
            "Send it short.",
        ] {
            assert_eq!(
                check_content(rule).expect_err(rule).code,
                "invalid_arguments",
                "{rule}"
            );
        }
        assert!(check_content("Frases cortas y sin punto y coma.").is_ok());
    }

    #[test]
    fn a_number_is_read_only_from_a_rule_line() {
        assert_eq!(rule_number("12. Una regla."), Some(12));
        assert_eq!(rule_number("# Cómo escribe. Algo."), None);
        assert_eq!(rule_number("Sin número."), None);
    }
}
