//! The mail draft card and the retry rules of the send question (MAILUX-HOST §A
//! and §C). A draft shows as one card per conversation and is refreshed in place.
//! A question that expired may be asked again; a refused one may not, until the
//! draft changes.

mod common;

use std::sync::Arc;

use common::{team, Team};
use excubia_chat::{
    action_digest, ApprovalKind, ApprovalStatus, ChatError, Decision, MailDraftCard, ManualClock,
    NewApproval, TimelineEntry, DEFAULT_TTL_SECONDS,
};
use serde_json::json;

const START_MS: i64 = 1_000_000;

/// A send question for a draft with `subject`: another subject is other content.
fn send_question(team: &Team, subject: &str) -> NewApproval {
    let args = json!({ "cuenta": "trabajo", "borrador": "draft-1", "asunto": subject });
    let cwd = team.root.to_string_lossy().to_string();
    let paths: [String; 0] = [];
    NewApproval {
        conversation_id: Some(team.conversation.clone()),
        seat_id: None,
        agent_label: None,
        run_id: None,
        pane_id: None,
        kind: ApprovalKind::Tool,
        tool: "mail.send".to_string(),
        summary: "Enviar el borrador".to_string(),
        cwd: Some(cwd.clone()),
        paths: paths.to_vec(),
        risk: None,
        action_digest: action_digest("tool", "mail.send", &args, &cwd, &paths),
        payload_redacted: Some(args),
        ttl_seconds: DEFAULT_TTL_SECONDS,
    }
}

/// The draft cards of the conversation, as the timeline lists them.
fn draft_cards(team: &Team) -> Vec<TimelineEntry> {
    team.store
        .timeline(&team.conversation, None, 50)
        .expect("timeline")
        .entries
        .into_iter()
        .filter(|entry| entry.kind == "mail_draft")
        .collect()
}

#[test]
fn a_draft_card_shows_once_per_conversation_and_refreshes_in_place() {
    let team = team("draft-card-refresh");
    let to = vec!["ana@example.org".to_string()];
    let card = |subject| MailDraftCard {
        account: "trabajo",
        draft_id: "draft-1",
        to: &to,
        subject,
        preview: "Hola Ana",
    };

    team.store
        .put_mail_draft_card(Some(&team.conversation), card("Presupuesto"))
        .expect("the card is shown");
    team.store
        .put_mail_draft_card(Some(&team.conversation), card("Presupuesto v2"))
        .expect("the card is shown again");
    team.store
        .put_mail_draft_card(None, card("Presupuesto v3"))
        .expect("the cards are refreshed");

    let cards = draft_cards(&team);
    assert_eq!(cards.len(), 1, "{cards:?}");
    assert_eq!(cards[0].account.as_deref(), Some("trabajo"));
    assert_eq!(cards[0].draft_id.as_deref(), Some("draft-1"));
    assert_eq!(cards[0].to, Some(to));
    assert_eq!(cards[0].subject.as_deref(), Some("Presupuesto v3"));
    assert_eq!(cards[0].preview.as_deref(), Some("Hola Ana"));
}

#[test]
fn a_card_for_a_conversation_that_does_not_exist_is_refused() {
    let team = team("draft-card-missing");
    let to = vec!["ana@example.org".to_string()];

    let refused = team.store.put_mail_draft_card(
        Some("cnv_missing"),
        MailDraftCard {
            account: "trabajo",
            draft_id: "draft-1",
            to: &to,
            subject: "Presupuesto",
            preview: "Hola Ana",
        },
    );

    assert!(refused.is_err());
    assert!(draft_cards(&team).is_empty());
}

#[test]
fn an_expired_question_can_be_asked_again() {
    let team = team("draft-retry-expired");
    let clock = Arc::new(ManualClock::at(START_MS));
    team.store.set_clock(clock.clone());
    let first = team
        .store
        .create_mail_send_approval(send_question(&team, "Presupuesto"))
        .expect("the question is filed");

    clock.advance_secs(i64::from(DEFAULT_TTL_SECONDS) + 1);
    let again = team
        .store
        .create_mail_send_approval(send_question(&team, "Presupuesto"))
        .expect("the same draft is asked again after it expired");

    assert_ne!(again.id, first.id);
    assert_eq!(again.status, ApprovalStatus::Pending);
}

#[test]
fn a_refused_draft_must_change_before_it_is_asked_again() {
    let team = team("draft-retry-denied");
    let first = team
        .store
        .create_mail_send_approval(send_question(&team, "Presupuesto"))
        .expect("the question is filed");
    team.store
        .decide_approval(&first.id, Decision::Deny, "device-1", "device-1")
        .expect("the device refuses");

    let refused = team
        .store
        .create_mail_send_approval(send_question(&team, "Presupuesto"))
        .expect_err("the same content is refused");
    assert!(
        matches!(refused, ChatError::ApprovalDigestRefused),
        "{refused:?}"
    );

    let changed = team
        .store
        .create_mail_send_approval(send_question(&team, "Presupuesto corregido"))
        .expect("changed content is asked");
    assert_eq!(changed.status, ApprovalStatus::Pending);
}
