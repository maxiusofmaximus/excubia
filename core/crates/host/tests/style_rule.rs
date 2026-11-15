//! `style.propose_rule` (RFC estilo-aprendido §3, F2, ESTILO3, revisada 2026-11-14) over the real
//! chat store. A line is written into `estilo.md` at once, with a copy of the old profile in
//! `historial/`, and no card is filed. A line the profile already says is not written again. A
//! line the host cannot keep (an address, a link, a phone, a word about tools or sending, a 41st
//! rule, a profile past 4 KiB) is refused and nothing changes. An old card approved before the
//! change is still applied once.

mod mail_move_support;
mod mail_support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Local;
use excubia_chat::{
    Approval, ApprovalFilter, ApprovalKind, ApprovalStatus, ChatStore, Decision, NewApproval,
};
use excubia_host::mail_tools::MailTools;
use excubia_seat::StyleProposal;
use excubia_test_dir::TestDir;
use mail_move_support::{actor, code_of, host, scripted};
use serde_json::{json, Value};

const RULE: &str = "Trata de usted.";

fn profile(dir: &Path) -> PathBuf {
    dir.join("data/estilo/estilo.md")
}

fn line(rule: &str, source: &str) -> StyleProposal {
    StyleProposal {
        rule: rule.to_owned(),
        source: source.to_owned(),
    }
}

/// The cards still waiting for the operator.
fn open_cards(chat: &ChatStore) -> Vec<Approval> {
    chat.approvals(&ApprovalFilter {
        status: Some(ApprovalStatus::Pending),
        conversation_id: None,
        limit: 50,
    })
    .expect("list")
}

/// Proposes `line` and returns the result the answer carries.
async fn propose(tools: &MailTools, line: &StyleProposal) -> Value {
    tools
        .style_propose(&actor(), line)
        .await
        .expect("the answer")
        .result
}

/// Files a card the way the host did before this change, to test the cards still waiting.
fn old_card(chat: &ChatStore, rule: &str, source: &str) -> String {
    chat.create_style_rule_approval(NewApproval {
        conversation_id: None,
        seat_id: None,
        agent_label: None,
        run_id: None,
        pane_id: None,
        kind: ApprovalKind::Tool,
        tool: "style.propose_rule".to_owned(),
        summary: format!("Regla de estilo «{rule}» (origen: {source})"),
        cwd: None,
        paths: Vec::new(),
        risk: None,
        action_digest: format!("sha256:old-{rule}"),
        payload_redacted: Some(json!({ "rule": rule, "source": source, "question": rule })),
        ttl_seconds: 1800,
    })
    .expect("the old card is filed")
    .id
}

fn text_of(tools: &MailTools) -> String {
    tools.style().expect("style.get").result["text"]
        .as_str()
        .expect("the text")
        .to_owned()
}

fn history_copies(dir: &Path) -> Vec<String> {
    let mut copies: Vec<String> = std::fs::read_dir(dir.join("data/estilo/historial"))
        .expect("historial exists")
        .map(|entry| std::fs::read_to_string(entry.expect("entry").path()).expect("copy"))
        .collect();
    copies.sort();
    copies
}

#[tokio::test]
async fn a_proposal_is_saved_at_once_with_its_origin_and_date_and_a_copy_and_no_card() {
    let dir = TestDir::new("style-rule-saved");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let tools = host(&dir, &scripted(Vec::new()), &chat);
    let before = std::fs::read_to_string(profile(&dir)).expect("seeded");

    let answer = propose(&tools, &line(RULE, "correccion")).await;

    assert_eq!(answer["status"], "saved");
    let today = Local::now().format("%Y-%m-%d").to_string();
    assert_eq!(
        std::fs::read_to_string(profile(&dir)).expect("read"),
        format!("{before}5. {RULE} (origen: correccion, {today})\n")
    );
    assert_eq!(history_copies(&dir), vec![before]);
    assert!(open_cards(&chat).is_empty(), "no card is filed");
}

#[tokio::test]
async fn the_same_line_again_in_other_case_or_accent_is_already_saved_and_nothing_changes() {
    let dir = TestDir::new("style-rule-again");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let tools = host(&dir, &scripted(Vec::new()), &chat);
    propose(&tools, &line(RULE, "correccion")).await;
    let saved = std::fs::read_to_string(profile(&dir)).expect("read");

    let answer = propose(&tools, &line("trata de Ústed.", "manual")).await;

    assert_eq!(answer["status"], "already_saved");
    assert_eq!(std::fs::read_to_string(profile(&dir)).expect("read"), saved);
    assert_eq!(history_copies(&dir).len(), 1, "no second copy");
}

#[tokio::test]
async fn five_proposals_in_a_row_are_five_numbered_rules_and_no_card() {
    let dir = TestDir::new("style-rule-five");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let tools = host(&dir, &scripted(Vec::new()), &chat);
    let mut expected = std::fs::read_to_string(profile(&dir)).expect("seeded");
    let today = Local::now().format("%Y-%m-%d").to_string();

    let rules = [
        "Frases cortas.",
        "Trata de usted.",
        "Sin jerga.",
        "Cierra con un saludo.",
        "Pon el asunto claro.",
    ];
    for (n, rule) in rules.iter().enumerate() {
        assert_eq!(
            propose(&tools, &line(rule, "manual")).await["status"],
            "saved"
        );
        expected.push_str(&format!("{}. {rule} (origen: manual, {today})\n", n + 5));
    }

    assert_eq!(
        std::fs::read_to_string(profile(&dir)).expect("read"),
        expected
    );
    assert_eq!(history_copies(&dir).len(), 5);
    assert!(open_cards(&chat).is_empty(), "no card is filed");
}

#[tokio::test]
async fn an_old_card_approved_before_the_change_is_written_once_when_the_file_is_read() {
    let dir = TestDir::new("style-rule-old-approve");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let tools = host(&dir, &scripted(Vec::new()), &chat);
    let before = std::fs::read_to_string(profile(&dir)).expect("seeded");

    let id = old_card(&chat, RULE, "correccion");
    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");

    let today = Local::now().format("%Y-%m-%d").to_string();
    let expected = format!("{before}5. {RULE} (origen: correccion, {today})\n");
    assert_eq!(text_of(&tools), expected);
    assert_eq!(text_of(&tools), expected, "a second read adds nothing");
    assert_eq!(
        chat.approval(&id).expect("card").status,
        ApprovalStatus::Consumed
    );
    let copies = history_copies(&dir);
    assert_eq!(copies.len(), 1);
    assert!(copies.contains(&before));
}

#[tokio::test]
async fn an_old_card_denied_is_never_written() {
    let dir = TestDir::new("style-rule-old-deny");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let tools = host(&dir, &scripted(Vec::new()), &chat);
    let before = std::fs::read_to_string(profile(&dir)).expect("seeded");

    let id = old_card(&chat, RULE, "manual");
    chat.decide_approval(&id, Decision::Deny, "device-1", "device-1")
        .expect("decide");

    assert_eq!(text_of(&tools), before);
    assert_eq!(
        std::fs::read_to_string(profile(&dir)).expect("read"),
        before
    );
    assert!(
        !dir.join("data/estilo/historial").exists(),
        "nothing was saved, nothing copied"
    );
}

#[tokio::test]
async fn an_old_approved_card_that_no_longer_fits_is_consumed_and_not_written() {
    let dir = TestDir::new("style-rule-no-longer-fits");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let tools = host(&dir, &scripted(Vec::new()), &chat);
    let id = old_card(&chat, RULE, "manual");

    // The profile fills up while the card is open.
    let full: String = (1..=40).map(|n| format!("{n}. Regla {n}.\n")).collect();
    std::fs::write(profile(&dir), &full).expect("fill the profile");
    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");

    assert_eq!(text_of(&tools), full);
    assert_eq!(std::fs::read_to_string(profile(&dir)).expect("read"), full);
    assert_eq!(
        chat.approval(&id).expect("card").status,
        ApprovalStatus::Consumed
    );
    assert!(!dir.join("data/estilo/historial").exists());
}

#[tokio::test]
async fn an_old_approved_card_is_written_when_the_host_starts_again() {
    let dir = TestDir::new("style-rule-restart");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let tools = host(&dir, &scripted(Vec::new()), &chat);
    let before = std::fs::read_to_string(profile(&dir)).expect("seeded");
    let id = old_card(&chat, RULE, "manual");
    chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
        .expect("decide");
    drop(tools);

    // The approval waited while the host was down; the start writes it.
    let _restarted = host(&dir, &scripted(Vec::new()), &chat);

    let today = Local::now().format("%Y-%m-%d").to_string();
    assert_eq!(
        std::fs::read_to_string(profile(&dir)).expect("read"),
        format!("{before}5. {RULE} (origen: manual, {today})\n")
    );
}

#[tokio::test]
async fn a_line_with_an_address_a_link_a_phone_or_a_sending_word_is_refused_and_nothing_changes() {
    let dir = TestDir::new("style-rule-content");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let tools = host(&dir, &scripted(Vec::new()), &chat);
    let before = std::fs::read_to_string(profile(&dir)).expect("seeded");

    for rule in [
        "Escribe a juan@example.com.",
        "Mira https://example.es/a.",
        "Llama al 600 123 456.",
        "Envía siempre en PDF.",
    ] {
        let answer = tools.style_propose(&actor(), &line(rule, "manual")).await;
        assert_eq!(code_of(answer), "invalid_arguments", "{rule}");
    }
    assert_eq!(
        std::fs::read_to_string(profile(&dir)).expect("read"),
        before
    );
    assert!(
        !dir.join("data/estilo/historial").exists(),
        "nothing was copied"
    );
    assert!(open_cards(&chat).is_empty(), "no card for a refused line");
}

#[tokio::test]
async fn a_profile_that_would_pass_4_kib_is_refused_and_the_file_stays() {
    let dir = TestDir::new("style-rule-large");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let tools = host(&dir, &scripted(Vec::new()), &chat);
    let big = format!("{}\n", "x".repeat(4000));
    std::fs::write(profile(&dir), &big).expect("fill the profile");

    let answer = tools
        .style_propose(&actor(), &line(&"Usa frases cortas. ".repeat(12), "manual"))
        .await;

    assert_eq!(code_of(answer), "style_too_large");
    assert_eq!(std::fs::read_to_string(profile(&dir)).expect("read"), big);
    assert!(
        !dir.join("data/estilo/historial").exists(),
        "nothing was copied"
    );
}

#[tokio::test]
async fn a_forty_first_rule_is_refused_and_the_file_stays() {
    let dir = TestDir::new("style-rule-many");
    let chat = Arc::new(ChatStore::in_memory().expect("chat"));
    let tools = host(&dir, &scripted(Vec::new()), &chat);
    let full: String = (1..=40).map(|n| format!("{n}. Regla {n}.\n")).collect();
    std::fs::write(profile(&dir), &full).expect("fill the profile");

    let answer = tools.style_propose(&actor(), &line(RULE, "manual")).await;

    assert_eq!(code_of(answer), "style_too_many");
    assert_eq!(std::fs::read_to_string(profile(&dir)).expect("read"), full);
}
