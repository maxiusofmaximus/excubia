//! The pairing notice: a device that gets in (by code or by recovery code) leaves a
//! message from Alfred in the conversation «Avisos del sistema». The audit file stays
//! the record; the notice only tells the operator where to look. Refusals post nothing,
//! so a stranger's failed guesses never flood the chat.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use excubia_api::{PairOutcome, PairingAttempt, PairingAudit};
use excubia_chat::{ChatError, ChatStore, ConversationFilter, NewConversation};

use crate::cli::BIN;
use crate::pairing_audit::JsonlPairingAudit;

pub const NOTICE_TITLE: &str = "Avisos del sistema";
const LABEL_CHARS: usize = 40;
const PAGE: usize = 100;

pub struct PairingNotice {
    sink: JsonlPairingAudit,
    chat: Arc<ChatStore>,
    /// The notices' conversation, found or created by the first notice.
    conversation: Mutex<Option<String>>,
}

impl PairingNotice {
    pub fn new(data_dir: &Path, chat: Arc<ChatStore>) -> Self {
        Self {
            sink: JsonlPairingAudit::new(data_dir),
            chat,
            conversation: Mutex::new(None),
        }
    }

    /// Posts a message from Alfred in «Avisos del sistema», the host's notice conversation.
    pub fn notify(&self, text: &str) -> Result<(), ChatError> {
        let conversation = self.notice_conversation()?;
        self.chat
            .append_alfred_message(&conversation, text)
            .map(|_| ())
    }

    /// Found once and then kept: the lock covers the lookup, so two pairings at once
    /// never create two conversations.
    fn notice_conversation(&self) -> Result<String, ChatError> {
        let mut cached = self
            .conversation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(id) = cached.as_ref() {
            return Ok(id.clone());
        }
        let id = match self.find_notice_conversation()? {
            Some(id) => id,
            None => {
                self.chat
                    .create_conversation(NewConversation {
                        title: Some(NOTICE_TITLE.to_owned()),
                        project_id: None,
                    })?
                    .id
            }
        };
        *cached = Some(id.clone());
        Ok(id)
    }

    /// Among the conversations not archived, the one with the notices' title.
    fn find_notice_conversation(&self) -> Result<Option<String>, ChatError> {
        let mut cursor = None;
        loop {
            let page = self.chat.list_conversations(ConversationFilter {
                q: None,
                section_id: None,
                pinned: false,
                archived: false,
                attention: false,
                working: false,
                cursor: cursor.take(),
                limit: PAGE,
            })?;
            if let Some(found) = page
                .conversations
                .iter()
                .find(|conversation| conversation.title == NOTICE_TITLE)
            {
                return Ok(Some(found.id.clone()));
            }
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => return Ok(None),
            }
        }
    }
}

impl PairingAudit for PairingNotice {
    fn record(&self, attempt: &PairingAttempt) {
        self.sink.record(attempt);
        let Some(text) = notice_text(attempt) else {
            return;
        };
        // The notice is a courtesy: its failure is reported and never fails the pairing.
        if let Err(error) = self.notify(&text) {
            eprintln!("host: could not write the pairing notice: {error}");
        }
    }
}

fn notice_text(attempt: &PairingAttempt) -> Option<String> {
    // A refusal, a PAKE start and a failed PAKE post nothing: a stranger's tries never flood the chat, and the
    // device that completes the pairing is announced by its own `paired` line.
    if matches!(
        attempt.outcome,
        PairOutcome::Rejected | PairOutcome::PakeStarted | PairOutcome::PakeFailed
    ) {
        return None;
    }
    let device = attempt.device_id.as_deref()?;
    let label = label_span(attempt.label.as_deref().unwrap_or_default());
    let revoke = format!("Si no fuiste tú: `{BIN} devices revoke {device}`.");
    match attempt.outcome {
        PairOutcome::Paired => {
            // Without a pin there is no string to check, so the notice leaves it out.
            let verify = attempt
                .sas
                .as_ref()
                .map(|sas| {
                    format!(
                        " Cadena de verificación: **{sas}** (debe ser la que muestra el móvil)."
                    )
                })
                .unwrap_or_default();
            Some(format!(
                "Nuevo dispositivo pendiente: {label} ({device}), sin permisos.{verify} {revoke}"
            ))
        }
        PairOutcome::Recovered => Some(format!(
            "Se usó el código de recuperación de {device} ({label}). {revoke}"
        )),
        PairOutcome::Granted => {
            let granter = label_span(attempt.granted_by.as_deref().unwrap_or_default());
            let permissions = attempt.permissions.join(", ");
            Some(format!(
                "{granter} concedió {permissions} a {label} ({device})."
            ))
        }
        PairOutcome::Rejected | PairOutcome::PakeStarted | PairOutcome::PakeFailed => None,
    }
}

/// The label as an inline code span: one line, at most 40 characters, and no backtick
/// that could close the span early. Markdown inside a code span is plain text.
fn label_span(label: &str) -> String {
    // A PAKE pairing sends no label: say so instead of showing an empty span.
    if label.trim().is_empty() {
        return "`sin nombre`".to_owned();
    }
    let text: String = label
        .chars()
        .map(|character| match character {
            '`' => '\'',
            character if character.is_control() => ' ',
            character => character,
        })
        .take(LABEL_CHARS)
        .collect();
    format!("`{text}`")
}

#[cfg(test)]
mod tests {
    use super::*;
    use excubia_api::PairVia;

    fn attempt(outcome: PairOutcome, device: Option<&str>, label: Option<&str>) -> PairingAttempt {
        PairingAttempt::new(outcome, PairVia::Code, device, label, "127.0.0.1")
    }

    #[test]
    fn a_new_device_notice_carries_the_verification_string() {
        let mut paired = attempt(PairOutcome::Paired, Some("dev-1"), Some("Phone"));
        paired.sas = Some("042817".to_owned());
        assert_eq!(
            notice_text(&paired).expect("a notice"),
            format!(
                "Nuevo dispositivo pendiente: `Phone` (dev-1), sin permisos. \
                 Cadena de verificación: **042817** (debe ser la que muestra el móvil). \
                 Si no fuiste tú: `{BIN} devices revoke dev-1`."
            )
        );
    }

    #[test]
    fn a_grant_notice_names_the_granter_and_the_permissions() {
        let mut granted = attempt(PairOutcome::Granted, Some("dev-2"), Some("Tablet"));
        granted.granted_by = Some("dev-1".to_owned());
        granted.permissions = vec!["read_state".to_owned(), "respond".to_owned()];
        assert_eq!(
            notice_text(&granted).expect("a notice"),
            "`dev-1` concedió read_state, respond a `Tablet` (dev-2)."
        );
    }

    #[test]
    fn a_hostile_label_stays_one_short_line_of_code() {
        let label = format!("evil`\n## <b>x</b> {}", "y".repeat(60));
        let text = notice_text(&attempt(PairOutcome::Paired, Some("dev-1"), Some(&label)))
            .expect("a notice");

        assert!(!text.contains('\n'));
        let span = text.split('`').nth(1).expect("the label span");
        assert!(span.chars().count() <= LABEL_CHARS, "{span}");
        assert!(!span.contains('`'));
        assert!(text.starts_with("Nuevo dispositivo pendiente: `evil'"));
    }

    #[test]
    fn recovery_and_pairing_each_name_the_device_to_revoke() {
        let recovered = notice_text(&attempt(
            PairOutcome::Recovered,
            Some("dev-2"),
            Some("Pixel"),
        ))
        .expect("a notice");
        assert_eq!(
            recovered,
            format!(
                "Se usó el código de recuperación de dev-2 (`Pixel`). \
                 Si no fuiste tú: `{BIN} devices revoke dev-2`."
            )
        );
    }

    #[test]
    fn a_refusal_posts_nothing() {
        assert!(notice_text(&attempt(PairOutcome::Rejected, None, Some("Intruso"))).is_none());
    }

    #[test]
    fn a_pake_start_or_failure_posts_nothing() {
        for outcome in [PairOutcome::PakeStarted, PairOutcome::PakeFailed] {
            let mut pake = attempt(outcome, Some("dev-1"), None);
            pake.via = PairVia::Pake;
            assert!(notice_text(&pake).is_none(), "{outcome:?}");
        }
    }

    #[test]
    fn a_device_paired_by_pake_is_announced_like_any_other_with_its_verification_string() {
        let mut by_code = attempt(PairOutcome::Paired, Some("dev-1"), Some("Phone"));
        by_code.sas = Some("042817".to_owned());
        let mut by_pake = attempt(PairOutcome::Paired, Some("dev-1"), Some("Phone"));
        by_pake.via = PairVia::Pake;
        by_pake.sas = Some("042817".to_owned());
        assert_eq!(notice_text(&by_pake), notice_text(&by_code));

        // A PAKE device has no label yet.
        let mut unnamed = attempt(PairOutcome::Paired, Some("dev-2"), None);
        unnamed.via = PairVia::Pake;
        unnamed.sas = Some("123456".to_owned());
        assert_eq!(
            notice_text(&unnamed).expect("a notice"),
            format!(
                "Nuevo dispositivo pendiente: `sin nombre` (dev-2), sin permisos. Cadena de verificación: **123456** (debe ser la que muestra el móvil). Si no fuiste tú: `{BIN} devices revoke dev-2`."
            )
        );
    }
}
