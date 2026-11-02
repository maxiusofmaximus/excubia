//! The pairing notice: a device that gets in (by code or by recovery code) leaves a
//! message from Alfred in the conversation «Avisos del sistema». The audit file stays
//! the record; the notice only tells the operator where to look. Refusals post nothing,
//! so a stranger's failed guesses never flood the chat.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use hn_core_api::{PairOutcome, PairingAttempt, PairingAudit};
use hn_core_chat::{ChatError, ChatStore, ConversationFilter, NewConversation};

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

    fn notify(&self, text: &str) -> Result<(), ChatError> {
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
    if attempt.outcome == PairOutcome::Rejected {
        return None;
    }
    let device = attempt.device_id.as_deref()?;
    let label = label_span(attempt.label.as_deref().unwrap_or_default());
    let revoke = format!("Si no fuiste tú: `host devices revoke {device}`.");
    match attempt.outcome {
        PairOutcome::Paired => Some(format!(
            "Nuevo dispositivo pendiente: {label} ({device}), sin permisos. {revoke}"
        )),
        PairOutcome::Recovered => Some(format!(
            "Se usó el código de recuperación de {device} ({label}). {revoke}"
        )),
        PairOutcome::Rejected => None,
    }
}

/// The label as an inline code span: one line, at most 40 characters, and no backtick
/// that could close the span early. Markdown inside a code span is plain text.
fn label_span(label: &str) -> String {
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
    use hn_core_api::PairVia;

    fn attempt(outcome: PairOutcome, device: Option<&str>, label: Option<&str>) -> PairingAttempt {
        PairingAttempt {
            ts: 0.0,
            outcome,
            device_id: device.map(str::to_owned),
            label: label.map(str::to_owned),
            via: PairVia::Code,
            source_ip: "127.0.0.1".to_owned(),
        }
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
            "Se usó el código de recuperación de dev-2 (`Pixel`). \
             Si no fuiste tú: `host devices revoke dev-2`."
        );
    }

    #[test]
    fn a_refusal_posts_nothing() {
        assert!(notice_text(&attempt(PairOutcome::Rejected, None, Some("Intruso"))).is_none());
    }
}
