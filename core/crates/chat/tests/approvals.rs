//! Store-level acceptance tests of the approvals: the lifecycle, the TTL, the
//! single use, the idempotency of the decision, the replay protection and the
//! audit trail.

use std::path::PathBuf;
use std::time::Duration;

use hn_core_chat::{
    action_digest, ApprovalFilter, ApprovalKind, ApprovalStatus, ChatError, ChatStore, Decision,
    HookScope, NewApproval, NewConversation, Risk, DEFAULT_TTL_SECONDS,
    MAX_PENDING_PER_CONVERSATION,
};
use hn_core_test_dir::TestDir;

fn temp_dir(name: &str) -> TestDir {
    TestDir::new(&format!("core-approval-tests-{name}"))
}

/// The store, its file and the scratch directory that holds both.
fn store(name: &str) -> (ChatStore, PathBuf, TestDir) {
    let dir = temp_dir(name);
    let path = dir.join("chat.db");
    let store = ChatStore::open(&path).expect("open store");
    (store, path, dir)
}

fn conversation(store: &ChatStore, title: &str) -> String {
    store
        .create_conversation(NewConversation {
            title: Some(title.to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id
}

/// The seeded Alfred seat is the owner of these approvals: the scope of a
/// panel covers its whole seat.
fn scope(store: &ChatStore) -> HookScope {
    HookScope::seat(&seat(store))
}

fn seat(store: &ChatStore) -> String {
    store.bootstrap().expect("bootstrap").alfred.agent_id
}

/// One approval for one action. The digest is the identity of the action, so
/// every test needs its own.
fn request(store: &ChatStore, conversation_id: &str, action: &str) -> NewApproval {
    let args = serde_json::json!({ "command": action });
    NewApproval {
        conversation_id: Some(conversation_id.to_string()),
        seat_id: Some(seat(store)),
        agent_label: Some("bob".to_string()),
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

#[test]
fn the_full_cycle_is_pending_approved_and_consumed() {
    let (store, _path, _dir) = store("cycle");
    let conversation_id = conversation(&store, "Trabajo");
    let approval = store
        .create_approval(request(&store, &conversation_id, "cargo test"))
        .expect("approval");
    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.kind, ApprovalKind::Command);
    assert_eq!(approval.risk, Risk::High, "a command is high risk");
    assert_eq!(
        approval.conversation_id.as_deref(),
        Some(conversation_id.as_str())
    );

    let decided = store
        .decide_approval(&approval.id, Decision::Approve, "device-1", "device-1")
        .expect("decision");
    assert_eq!(decided.status, ApprovalStatus::Approved);
    assert_eq!(decided.decided_by.as_deref(), Some("device-1"));

    let consumed = store
        .consume_approval(&approval.id, None, &scope(&store))
        .expect("consume");
    assert_eq!(consumed.status, ApprovalStatus::Consumed);

    // A second consumption is a conflict: the action never runs twice.
    let error = store
        .consume_approval(&approval.id, None, &scope(&store))
        .expect_err("consumed twice");
    assert!(
        matches!(error, ChatError::ApprovalAlreadyDecided),
        "{error}"
    );
}

#[test]
fn another_run_cannot_touch_the_approval() {
    let (store, _path, _dir) = store("scope");
    let conversation_id = conversation(&store, "Trabajo");
    let mut new = request(&store, &conversation_id, "cargo test");
    new.run_id = Some("run_a".to_string());
    let approval = store.create_approval(new).expect("approval");
    store
        .decide_approval(&approval.id, Decision::Approve, "device-1", "device-1")
        .expect("decision");

    let other_run = HookScope::run("run_b", &conversation_id, &seat(&store));
    assert!(matches!(
        store.approval_state(&approval.id, &other_run),
        Err(ChatError::HookScopeMismatch)
    ));
    assert!(matches!(
        store.consume_approval(&approval.id, None, &other_run),
        Err(ChatError::HookScopeMismatch)
    ));
    assert_eq!(
        store.approval(&approval.id).expect("approval").status,
        ApprovalStatus::Approved,
        "a foreign token changes nothing"
    );

    // The token of the run that owns it still reads it.
    let owner = HookScope::run("run_a", &conversation_id, &seat(&store));
    assert_eq!(
        store
            .approval_state(&approval.id, &owner)
            .expect("state")
            .status,
        ApprovalStatus::Approved
    );
}

#[test]
fn a_denied_approval_never_runs() {
    let (store, _path, _dir) = store("denied");
    let conversation_id = conversation(&store, "Trabajo");
    let approval = store
        .create_approval(request(&store, &conversation_id, "rm -rf"))
        .expect("approval");
    let denied = store
        .decide_approval(&approval.id, Decision::Deny, "device-1", "device-1")
        .expect("decision");
    assert_eq!(denied.status, ApprovalStatus::Denied);
    let error = store
        .consume_approval(&approval.id, None, &scope(&store))
        .expect_err("a denied approval cannot be consumed");
    assert!(matches!(error, ChatError::ApprovalNotApproved), "{error}");
}

#[test]
fn the_ttl_expires_the_approval() {
    let (store, _path, _dir) = store("ttl");
    let conversation_id = conversation(&store, "Trabajo");
    let mut new = request(&store, &conversation_id, "sleep 100");
    new.ttl_seconds = 1;
    let approval = store.create_approval(new).expect("approval");
    assert_eq!(approval.status, ApprovalStatus::Pending);

    std::thread::sleep(Duration::from_millis(1_200));
    let expired = store.approval(&approval.id).expect("approval");
    assert_eq!(expired.status, ApprovalStatus::Expired);
    let error = store
        .decide_approval(&approval.id, Decision::Approve, "device-1", "device-1")
        .expect_err("an expired approval cannot be decided");
    assert!(matches!(error, ChatError::ApprovalExpired), "{error}");
    assert_eq!(
        store.pending_approval_count().expect("counter"),
        0,
        "an expired approval is not pending any more"
    );
}

#[test]
fn the_maximum_ttl_is_three_hundred_seconds() {
    let (store, path, _dir) = store("ttl-max");
    let conversation_id = conversation(&store, "Trabajo");
    let mut new = request(&store, &conversation_id, "cargo build");
    new.ttl_seconds = 10_000;
    let approval = store.create_approval(new).expect("approval");

    // The host clamps the TTL: the deadline is at most 300 seconds away and
    // it is not cut to the minimum.
    let connection = rusqlite::Connection::open(&path).expect("raw");
    let horizon: String = connection
        .query_row(
            "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+300 seconds')",
            [],
            |row| row.get(0),
        )
        .expect("horizon");
    let floor: String = connection
        .query_row(
            "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+290 seconds')",
            [],
            |row| row.get(0),
        )
        .expect("floor");
    assert!(
        approval.expires_at <= horizon,
        "{} is further away than 300 s",
        approval.expires_at
    );
    assert!(
        approval.expires_at >= floor,
        "{} is closer than 290 s",
        approval.expires_at
    );
}

#[test]
fn a_digest_is_never_approved_twice() {
    let (store, _path, _dir) = store("replay");
    let conversation_id = conversation(&store, "Trabajo");
    let approval = store
        .create_approval(request(&store, &conversation_id, "cargo test"))
        .expect("approval");
    // The same seat asking again while it waits gets the same approval back.
    let again = store
        .create_approval(request(&store, &conversation_id, "cargo test"))
        .expect("the request is coalesced");
    assert_eq!(again.id, approval.id);

    // Even after the first one was consumed, the action stays spent.
    store
        .decide_approval(&approval.id, Decision::Approve, "device-1", "device-1")
        .expect("decision");
    store
        .consume_approval(&approval.id, None, &scope(&store))
        .expect("consume");
    let error = store
        .create_approval(request(&store, &conversation_id, "cargo test"))
        .expect_err("the action is spent");
    assert!(matches!(error, ChatError::ApprovalDigestReused), "{error}");
}

#[test]
fn an_expired_digest_is_never_reopened_and_is_not_coalesced() {
    let (store, _path, _dir) = store("replay-expired");
    let conversation_id = conversation(&store, "Trabajo");
    let mut new = request(&store, &conversation_id, "cargo build");
    new.ttl_seconds = 1;
    let approval = store.create_approval(new).expect("approval");

    std::thread::sleep(Duration::from_millis(1_200));
    assert_eq!(
        store.approval(&approval.id).expect("approval").status,
        ApprovalStatus::Expired
    );
    // The expired row still holds the digest: asking again is a refusal, not
    // a new pending approval, even from the seat that asked first.
    let error = store
        .create_approval(request(&store, &conversation_id, "cargo build"))
        .expect_err("an expired digest is spent");
    assert!(matches!(error, ChatError::ApprovalDigestReused), "{error}");
    assert_eq!(store.pending_approval_count().expect("counter"), 0);
}

#[test]
fn another_device_cannot_decide_and_the_same_one_is_idempotent() {
    let (store, _path, _dir) = store("idempotent");
    let conversation_id = conversation(&store, "Trabajo");
    let approval = store
        .create_approval(request(&store, &conversation_id, "cargo clippy"))
        .expect("approval");

    let first = store
        .decide_approval(&approval.id, Decision::Approve, "device-1", "device-1")
        .expect("decision");
    assert_eq!(first.status, ApprovalStatus::Approved);

    // The same device asking again gets the same answer.
    let again = store
        .decide_approval(&approval.id, Decision::Approve, "device-1", "device-1")
        .expect("idempotent");
    assert_eq!(again.status, ApprovalStatus::Approved);
    assert_eq!(again.decided_by.as_deref(), Some("device-1"));

    // Another device is a conflict.
    let error = store
        .decide_approval(&approval.id, Decision::Deny, "device-2", "device-2")
        .expect_err("another device");
    assert!(
        matches!(error, ChatError::ApprovalAlreadyDecided),
        "{error}"
    );
}

#[test]
fn the_audit_keeps_the_device_the_digest_and_the_hour() {
    let (store, _path, _dir) = store("audit");
    let conversation_id = conversation(&store, "Trabajo");
    let approval = store
        .create_approval(request(&store, &conversation_id, "cargo audit"))
        .expect("approval");
    store
        .decide_approval(&approval.id, Decision::Approve, "device-9", "device-9")
        .expect("decision");
    store
        .consume_approval(&approval.id, None, &scope(&store))
        .expect("consume");

    let audited = store.approval(&approval.id).expect("approval");
    assert_eq!(audited.audit.len(), 2, "{:?}", audited.audit);
    let decision = &audited.audit[0];
    assert_eq!(decision.device_id, "device-9");
    assert_eq!(decision.action_digest, audited.action_digest);
    assert_eq!(decision.decision, "approved");
    assert_eq!(decision.scope, "once");
    assert!(!decision.decided_at.is_empty());
    assert_eq!(audited.audit[1].decision, "consumed");
    assert_eq!(audited.audit[1].device_id, "hook");
}

#[test]
fn a_conversation_does_not_collect_approvals_without_limit() {
    let (store, _path, _dir) = store("limit");
    let conversation_id = conversation(&store, "Trabajo");
    for index in 0..MAX_PENDING_PER_CONVERSATION {
        store
            .create_approval(request(
                &store,
                &conversation_id,
                &format!("cargo test {index}"),
            ))
            .expect("approval");
    }
    let error = store
        .create_approval(request(&store, &conversation_id, "one too many"))
        .expect_err("the limit of the conversation");
    assert!(matches!(error, ChatError::ApprovalLimit), "{error}");
    assert_eq!(
        store.pending_approval_count().expect("counter"),
        MAX_PENDING_PER_CONVERSATION as u64
    );
}

#[test]
fn the_inbox_lists_the_pending_ones_and_filters_by_conversation() {
    let (store, _path, _dir) = store("inbox");
    let first = conversation(&store, "One");
    let second = conversation(&store, "Two");
    store
        .create_approval(request(&store, &first, "cargo one"))
        .expect("approval");
    store
        .create_approval(request(&store, &second, "cargo two"))
        .expect("approval");

    let pending = store
        .approvals(&ApprovalFilter {
            status: Some(ApprovalStatus::Pending),
            conversation_id: None,
            limit: 50,
        })
        .expect("inbox");
    assert_eq!(pending.len(), 2);

    let filtered = store
        .approvals(&ApprovalFilter {
            status: Some(ApprovalStatus::Pending),
            conversation_id: Some(second.clone()),
            limit: 50,
        })
        .expect("inbox");
    assert_eq!(filtered.len(), 1);
    assert_eq!(
        filtered[0].conversation_id.as_deref(),
        Some(second.as_str())
    );
}

#[test]
fn a_restart_expires_the_approvals_that_waited() {
    let dir = temp_dir("restart");
    let path = dir.join("chat.db");
    {
        let store = ChatStore::open(&path).expect("open store");
        let conversation_id = conversation(&store, "Trabajo");
        store
            .create_approval(request(&store, &conversation_id, "cargo test"))
            .expect("approval");
        assert_eq!(store.pending_approval_count().expect("counter"), 1);
    }
    let store = ChatStore::open(&path).expect("reopen");
    assert_eq!(
        store.pending_approval_count().expect("counter"),
        0,
        "an approval does not survive a restart"
    );
    let expired = store
        .approvals(&ApprovalFilter {
            status: Some(ApprovalStatus::Expired),
            conversation_id: None,
            limit: 50,
        })
        .expect("expired");
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].audit.len(), 1);
    assert_eq!(expired[0].audit[0].decision, "expired");
}
