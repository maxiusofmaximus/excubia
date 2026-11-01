use super::*;
use crate::chat::Quote;

fn message(seq: i64, author: &str, body: &str) -> Message {
    Message {
        id: format!("msg_{seq}"),
        conversation_id: "cnv_1".to_string(),
        seq,
        author_kind: author.to_string(),
        author_agent_id: None,
        body_md: body.to_string(),
        state: "complete".to_string(),
        turn_state: None,
        suggestions: Vec::new(),
        quote: None,
        attachments: Vec::new(),
        created_at: "2026-10-22T10:00:00.000Z".to_string(),
    }
}

fn card(seq: i64, delegation_id: &str, status: &str) -> TimelineEntry {
    let mut entry = timeline(seq, "card", None);
    entry.ref_id = Some(delegation_id.to_string());
    entry.delegation_id = Some(delegation_id.to_string());
    entry.status = Some(status.to_string());
    entry.title = Some(format!("delegación {delegation_id}"));
    entry
}

fn timeline(seq: i64, kind: &str, state: Option<&str>) -> TimelineEntry {
    TimelineEntry {
        seq,
        kind: kind.to_string(),
        message_id: Some(format!("msg_{seq}")),
        author_kind: None,
        author_agent_id: None,
        preview: Some("vista previa".to_string()),
        state: state.map(str::to_string),
        turn_state: None,
        suggestions: Vec::new(),
        quote: None,
        attachments: Vec::new(),
        card_id: None,
        delegation_id: None,
        ref_id: None,
        title: None,
        summary: None,
        status: None,
        agent_id: None,
        pane_id: None,
        attention: None,
        links: None,
        account: None,
        draft_id: None,
        to: Vec::new(),
        subject: None,
        created_at: "2026-10-22T10:00:00.000Z".to_string(),
    }
}

#[test]
fn pages_are_ordered_by_seq_even_when_they_arrive_out_of_order() {
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    feed.merge_messages([message(4, "agent", "cuatro"), message(2, "user", "dos")]);
    feed.merge_messages([message(3, "user", "tres"), message(1, "agent", "uno")]);
    let seqs: Vec<i64> = feed.snapshot().entries.iter().map(ChatEntry::seq).collect();
    assert_eq!(seqs, vec![1, 2, 3, 4]);
}

#[test]
fn a_reconnect_that_replays_the_same_page_does_not_duplicate() {
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    let page = vec![message(1, "user", "hola"), message(2, "agent", "hecho")];
    feed.merge_messages(page.clone());
    feed.merge_messages(page);
    assert_eq!(feed.snapshot().entries.len(), 2);
    assert_eq!(feed.last_seq(), 2);

    feed.merge_messages([message(3, "user", "otra")]);
    assert_eq!(feed.snapshot().entries.len(), 3);
}

#[test]
fn suggestions_and_quotes_come_from_the_timeline_and_survive_a_message_page() {
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    feed.merge_messages([
        message(1, "agent", "¿Qué nombre?"),
        message(2, "user", "Sí"),
    ]);
    let mut agent = timeline(1, "message", None);
    agent.suggestions = vec!["Sí, créalo".to_string(), "Cámbialo".to_string()];
    let mut own = timeline(2, "message", None);
    own.quote = Some(Quote {
        id: "msg_1".to_string(),
        author_kind: "agent".to_string(),
        preview: "¿Qué nombre?".to_string(),
    });
    feed.merge_timeline([agent, own]);
    // The next message page has neither field: the timeline's values stay.
    feed.merge_messages([message(1, "agent", "¿Qué nombre?")]);

    let entries = feed.snapshot().entries;
    match (&entries[0], &entries[1]) {
        (
            ChatEntry::Message { message: first, .. },
            ChatEntry::Message {
                message: second, ..
            },
        ) => {
            assert_eq!(first.suggestions, ["Sí, créalo", "Cámbialo"]);
            assert_eq!(
                second.quote.as_ref().map(|quote| quote.id.as_str()),
                Some("msg_1")
            );
        }
        other => panic!("se esperaban dos mensajes, llegó {other:?}"),
    }
}

#[test]
fn a_timeline_update_changes_the_state_without_losing_the_body() {
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    feed.merge_messages([message(1, "agent", "respuesta real")]);
    feed.merge_timeline([timeline(1, "message", Some("failed"))]);
    match &feed.snapshot().entries[0] {
        ChatEntry::Message { message, .. } => {
            assert_eq!(message.state, "failed");
            assert_eq!(message.body_md, "respuesta real");
        }
        other => panic!("esperaba un mensaje, llegó {other:?}"),
    }
}

#[test]
fn cards_and_unknown_kinds_survive_a_newer_host() {
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    feed.merge_timeline([timeline(2, "card", None), timeline(3, "mystery", None)]);
    let snapshot = feed.snapshot();
    assert!(matches!(snapshot.entries[0], ChatEntry::Card { .. }));
    assert!(matches!(snapshot.entries[1], ChatEntry::Unknown { .. }));
}

fn mail_draft(seq: i64, account: Option<&str>, draft_id: Option<&str>) -> TimelineEntry {
    let mut entry = timeline(seq, "mail_draft", None);
    entry.account = account.map(str::to_string);
    entry.draft_id = draft_id.map(str::to_string);
    entry.to = vec!["ana@example.com".to_string()];
    entry.subject = Some("Presupuesto".to_string());
    entry
}

#[test]
fn a_mail_draft_row_becomes_a_card_that_names_its_draft() {
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    feed.merge_timeline([
        mail_draft(2, Some("trabajo"), Some("drf_1")),
        timeline(3, "mystery", None),
    ]);
    let snapshot = feed.snapshot();
    match &snapshot.entries[0] {
        ChatEntry::MailDraft { seq: 2, entry } => {
            assert_eq!(entry.account, "trabajo");
            assert_eq!(entry.draft_id, "drf_1");
            assert_eq!(entry.to, vec!["ana@example.com".to_string()]);
            assert_eq!(entry.subject, "Presupuesto");
            assert_eq!(entry.preview, "vista previa");
        }
        other => panic!("expected a mail draft card, got {other:?}"),
    }
    assert!(matches!(snapshot.entries[1], ChatEntry::Unknown { .. }));
}

#[test]
fn a_mail_draft_row_without_its_draft_is_left_off_the_timeline() {
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    feed.merge_timeline([
        mail_draft(2, Some("trabajo"), None),
        mail_draft(3, Some("trabajo"), Some("")),
        mail_draft(4, None, Some("drf_2")),
        timeline(5, "mystery", None),
    ]);
    let snapshot = feed.snapshot();
    assert_eq!(snapshot.entries.len(), 1);
    assert!(matches!(
        snapshot.entries[0],
        ChatEntry::Unknown { seq: 5, .. }
    ));
}

#[test]
fn a_mail_draft_wire_row_parses_into_its_card() {
    let row = serde_json::json!({
        "seq": 6,
        "kind": "mail_draft",
        "card_id": "crd_1",
        "account": "trabajo",
        "draft_id": "drf_9",
        "to": ["ana@example.com"],
        "subject": "Hola",
        "preview": "Texto breve",
        "created_at": "2026-10-22T10:00:00.000Z",
    });
    let entry: TimelineEntry = serde_json::from_value(row).unwrap();
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    feed.merge_timeline([entry]);
    match &feed.snapshot().entries[0] {
        ChatEntry::MailDraft { seq: 6, entry } => {
            assert_eq!(entry.draft_id, "drf_9");
            assert_eq!(entry.preview, "Texto breve");
        }
        other => panic!("expected a mail draft card, got {other:?}"),
    }
}

#[test]
fn an_approval_line_is_its_own_entry_and_pending_approvals_ride_along() {
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    feed.merge_timeline([timeline(4, "approval", None)]);
    feed.set_approvals(vec![Approval {
        id: "apr_01J1".to_string(),
        level: Some("user".to_string()),
        ..Approval::default()
    }]);
    let snapshot = feed.snapshot();
    assert!(matches!(
        snapshot.entries[0],
        ChatEntry::Approval { seq: 4, .. }
    ));
    assert_eq!(snapshot.approvals.len(), 1);
    assert_eq!(snapshot.approvals[0].id, "apr_01J1");

    feed.set_approvals(Vec::new());
    assert_eq!(
        feed.snapshot().approvals.len(),
        1,
        "an approval that left the inbox keeps its card"
    );
}

#[test]
fn a_fresh_inbox_row_replaces_the_held_one() {
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    feed.set_approvals(vec![Approval {
        id: "apr_01J3".to_string(),
        status: "pending".to_string(),
        ..Approval::default()
    }]);
    feed.set_approvals(vec![Approval {
        id: "apr_01J3".to_string(),
        status: "expired".to_string(),
        ..Approval::default()
    }]);
    let approvals = feed.snapshot().approvals;
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0].status, "expired");
}

#[test]
fn a_reset_forgets_the_previous_conversation_approvals() {
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    feed.set_approvals(vec![Approval {
        id: "apr_01J2".to_string(),
        ..Approval::default()
    }]);
    feed.reset("cnv_2");
    assert!(feed.snapshot().approvals.is_empty());
}

#[test]
fn one_card_per_delegation_and_the_newest_state_wins() {
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    feed.merge_timeline([card(5, "dlg_1", "running")]);
    feed.merge_timeline([card(5, "dlg_1", "done")]);
    let snapshot = feed.snapshot();
    assert_eq!(snapshot.entries.len(), 1, "same seq replaces the card");
    match &snapshot.entries[0] {
        ChatEntry::Card { entry, .. } => assert_eq!(entry.status.as_deref(), Some("done")),
        other => panic!("esperaba tarjeta, llegó {other:?}"),
    }

    // An older page must not overwrite the terminal state.
    feed.merge_timeline([card(3, "dlg_1", "queued")]);
    let snapshot = feed.snapshot();
    assert_eq!(snapshot.entries.len(), 1);
    match &snapshot.entries[0] {
        ChatEntry::Card { seq, entry } => {
            assert_eq!(*seq, 5);
            assert_eq!(entry.status.as_deref(), Some("done"));
        }
        other => panic!("esperaba tarjeta, llegó {other:?}"),
    }

    // A newer seq for the same delegation replaces it, still one card.
    feed.merge_timeline([card(7, "dlg_1", "failed")]);
    let snapshot = feed.snapshot();
    assert_eq!(snapshot.entries.len(), 1);
    match &snapshot.entries[0] {
        ChatEntry::Card { seq, entry } => {
            assert_eq!(*seq, 7);
            assert_eq!(entry.status.as_deref(), Some("failed"));
        }
        other => panic!("esperaba tarjeta, llegó {other:?}"),
    }

    feed.merge_timeline([card(4, "dlg_2", "running")]);
    assert_eq!(feed.snapshot().entries.len(), 2);
}

#[test]
fn reopening_the_same_conversation_keeps_the_page_but_another_one_resets() {
    let mut feed = ChatFeed::default();
    feed.reset("cnv_1");
    feed.merge_messages([message(1, "user", "hola")]);
    feed.ensure("cnv_1");
    assert_eq!(feed.snapshot().entries.len(), 1);
    feed.ensure("cnv_2");
    assert_eq!(feed.snapshot().entries.len(), 0);
    assert_eq!(feed.conversation_id(), Some("cnv_2"));
}
