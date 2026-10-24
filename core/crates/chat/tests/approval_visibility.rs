//! Store-level tests of what an operator can see (APK8), of who decides an
//! approval (the signed device, `client_id` only as the idempotency key) and of
//! a turn the seat refused (withdrawn from the conversation).

use hn_core_chat::{
    action_digest, ApprovalFilter, ApprovalKind, ApprovalStatus, ChatError, ChatStore, Decision,
    NewApproval, NewConversation, DEFAULT_TTL_SECONDS,
};
use hn_core_test_dir::TestDir;

fn store(name: &str) -> (ChatStore, TestDir) {
    let dir = TestDir::new(&format!("core-approval-visibility-{name}"));
    let store = ChatStore::open(&dir.join("chat.db")).expect("open store");
    (store, dir)
}

fn conversation(store: &ChatStore) -> String {
    store
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

fn seat(store: &ChatStore) -> String {
    store.bootstrap().expect("bootstrap").alfred.agent_id
}

/// A seat-token request (no run, no pane) or a run's one, by `conversation_id`.
fn request(store: &ChatStore, conversation_id: Option<&str>, action: &str) -> NewApproval {
    let args = serde_json::json!({ "command": action });
    NewApproval {
        conversation_id: conversation_id.map(str::to_string),
        seat_id: Some(seat(store)),
        agent_label: Some("claude".to_string()),
        run_id: None,
        pane_id: None,
        kind: ApprovalKind::Command,
        tool: "bash".to_string(),
        summary: format!("Ejecutar comando: {action}"),
        cwd: Some("/work".to_string()),
        paths: Vec::new(),
        risk: None,
        action_digest: action_digest("command", "bash", &args, "/work", &[]),
        payload_redacted: Some(args),
        ttl_seconds: DEFAULT_TTL_SECONDS,
    }
}

fn approval_lines(store: &ChatStore, conversation_id: &str) -> Vec<hn_core_chat::TimelineEntry> {
    store
        .timeline(conversation_id, None, 100)
        .expect("timeline")
        .entries
        .into_iter()
        .filter(|entry| entry.kind == "approval")
        .collect()
}

#[test]
fn a_request_without_conversation_waits_at_the_operator() {
    let (store, _dir) = store("orphan");
    let approval = store
        .create_approval(request(&store, None, "cargo test"))
        .expect("approval");

    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.level.as_deref(), Some("user"));
    assert_eq!(
        store
            .bootstrap()
            .expect("bootstrap")
            .chat
            .pending_user_approvals,
        1
    );
    let inbox = store
        .approvals(&ApprovalFilter {
            status: Some(ApprovalStatus::Pending),
            conversation_id: None,
            limit: 50,
        })
        .expect("inbox");
    assert!(inbox.iter().any(|row| row.id == approval.id));
}

#[test]
fn a_conversation_line_is_born_pending_and_keeps_its_place() {
    let (store, _dir) = store("line");
    let conversation_id = conversation(&store);
    let approval = store
        .create_approval(request(&store, Some(&conversation_id), "cargo test"))
        .expect("approval");

    let pending = approval_lines(&store, &conversation_id);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].ref_id.as_deref(), Some(approval.id.as_str()));
    assert_eq!(pending[0].status.as_deref(), Some("pending"));
    let seq = pending[0].seq;

    store
        .decide_approval(&approval.id, Decision::Approve, "client-1", "device-1")
        .expect("decision");

    let decided = approval_lines(&store, &conversation_id);
    assert_eq!(
        decided.len(),
        1,
        "the decision rewrites the line, it adds none"
    );
    assert_eq!(decided[0].seq, seq);
    assert_eq!(decided[0].status.as_deref(), Some("approved"));
    assert_eq!(decided[0].decider_kind.as_deref(), Some("user"));
    assert_eq!(decided[0].decider_id.as_deref(), Some("device-1"));
}

#[test]
fn the_decider_is_the_device_and_the_client_is_only_the_idempotency_key() {
    let (store, _dir) = store("decider");
    let conversation_id = conversation(&store);
    let approval = store
        .create_approval(request(&store, Some(&conversation_id), "cargo test"))
        .expect("approval");

    let first = store
        .decide_approval(&approval.id, Decision::Approve, "client-1", "device-1")
        .expect("decision");
    assert_eq!(first.decided_by.as_deref(), Some("device-1"));

    let replay = store
        .decide_approval(&approval.id, Decision::Approve, "client-1", "device-1")
        .expect("the same client gets the same answer");
    assert_eq!(replay.status, ApprovalStatus::Approved);

    let other_client = store
        .decide_approval(&approval.id, Decision::Deny, "client-2", "device-2")
        .expect_err("another client over a resolved approval conflicts");
    assert!(matches!(other_client, ChatError::ApprovalAlreadyDecided));
}

#[test]
fn a_refused_turn_leaves_no_message_and_its_client_id_can_be_reused() {
    let (store, _dir) = store("withdraw");
    let conversation_id = conversation(&store);
    let accepted = store
        .append_user_message(&conversation_id, "client-7", "hola")
        .expect("message");

    store.withdraw_turn(&accepted.message.id).expect("withdraw");

    let page = store
        .timeline(&conversation_id, None, 50)
        .expect("timeline");
    assert!(page
        .entries
        .iter()
        .all(|entry| entry.message_id.as_deref() != Some(accepted.message.id.as_str())));

    let again = store
        .append_user_message(&conversation_id, "client-7", "hola")
        .expect("the same text is accepted again");
    assert!(!again.duplicate, "the withdrawn copy is not a duplicate");
}
