//! The host policy seen from the store (CORE9-A): what the rules decide at the
//! creation of an approval, the audit row they leave, and the reload of
//! `approvals.toml` next to `chat.db`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use hn_core_chat::{
    action_digest, Approval, ApprovalStatus, ChatStore, HookScope, NewApproval, NewConversation,
    Risk, DEFAULT_TTL_SECONDS,
};
use hn_core_test_dir::TestDir;
use serde_json::json;

/// A store with the seeded Alfred seat, optionally linked to a project whose
/// root is a real folder (the policy canonicalises paths, so it must exist).
struct Desk {
    store: ChatStore,
    dir: TestDir,
    root: PathBuf,
    seat: String,
    conversation: String,
}

fn desk(name: &str) -> Desk {
    let dir = TestDir::new(&format!("core-policy-tests-{name}"));
    let root = dir.join("project");
    std::fs::create_dir_all(&root).expect("project root");
    let store = ChatStore::open(&dir.join("chat.db")).expect("open store");
    let seat = store.bootstrap().expect("bootstrap").alfred.agent_id;
    let conversation = store
        .create_conversation(NewConversation {
            title: Some(name.to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id;
    Desk {
        store,
        dir,
        root,
        seat,
        conversation,
    }
}

/// Link the seat to a project whose root is the desk's `project` folder.
fn link_project(desk: &Desk) {
    let connection = rusqlite::Connection::open(desk.dir.join("chat.db")).expect("raw database");
    connection
        .execute(
            "INSERT INTO project (id, company_id, name, root_path, created_at)
             VALUES ('prj_demo', (SELECT id FROM company), 'Demo', ?1,
                     '2026-10-23T09:00:00.000Z')",
            [desk.root.to_string_lossy().as_ref()],
        )
        .expect("project");
    connection
        .execute(
            "UPDATE agent_seat SET project_id = 'prj_demo' WHERE id = ?1",
            [&desk.seat],
        )
        .expect("seat in project");
    // A live lead, so an approval that waits at its level has someone to wait for.
    connection
        .execute(
            "INSERT INTO agent_seat (id, company_id, project_id, role_id, name, status, created_at)
             VALUES ('ag_lead_demo', (SELECT id FROM company), 'prj_demo', 'lead', 'Lead',
                     'idle', '2026-10-23T09:00:00.000Z')",
            [],
        )
        .expect("lead in project");
}

impl Desk {
    fn ask(&self, command: &str, cwd: &Path, risk: Option<Risk>) -> Approval {
        let args = json!({ "command": command });
        let cwd = cwd.to_string_lossy().to_string();
        let request = NewApproval {
            conversation_id: Some(self.conversation.clone()),
            seat_id: Some(self.seat.clone()),
            agent_label: Some("alfred".to_string()),
            run_id: None,
            pane_id: None,
            kind: hn_core_chat::ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: format!("Ejecutar comando: {command}"),
            cwd: Some(cwd.clone()),
            paths: Vec::new(),
            risk,
            action_digest: action_digest("command", "bash", &args, &cwd, &[]),
            payload_redacted: Some(args),
            ttl_seconds: DEFAULT_TTL_SECONDS,
        };
        self.store.create_approval(request).expect("approval")
    }

    fn in_project(&self, command: &str) -> Approval {
        self.ask(command, &self.root, None)
    }
}

#[test]
fn an_allowed_test_is_approved_at_once_and_audited_as_the_policy() {
    let desk = desk("allowed");
    link_project(&desk);
    let approval = desk.in_project("cargo test -p hn-core-chat");

    assert_eq!(approval.status, ApprovalStatus::Approved);
    assert_eq!(approval.level.as_deref(), Some("policy"));
    assert_eq!(approval.decider_kind.as_deref(), Some("policy"));
    assert_eq!(approval.decider_id.as_deref(), Some("test-in-project"));
    assert!(approval.decided_at.is_some(), "decided at creation");
    assert_eq!(approval.audit.len(), 1);
    assert_eq!(approval.audit[0].device_id, "test-in-project");
    assert_eq!(approval.audit[0].decision, "approved");

    let consumed = desk
        .store
        .consume_approval(
            &approval.id,
            Some(approval.action_digest.as_str()),
            &HookScope::seat(&desk.seat),
        )
        .expect("the hook burns the policy's approval");
    assert_eq!(consumed.status, ApprovalStatus::Consumed);
    assert_eq!(desk.store.pending_approval_count().expect("count"), 0);
}

#[test]
fn a_read_inside_the_project_is_approved_too() {
    let desk = desk("read");
    link_project(&desk);
    let approval = desk.in_project("cat src/lib.rs");
    assert_eq!(approval.status, ApprovalStatus::Approved);
    assert_eq!(approval.decider_id.as_deref(), Some("read-in-project"));
}

#[test]
fn a_push_is_never_delegated_even_when_the_rules_would_allow_it() {
    let desk = desk("push");
    link_project(&desk);
    let approval = desk.in_project("git push origin main");

    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.level.as_deref(), Some("user"));
    assert_eq!(approval.escalated_from.as_deref(), Some("policy"));
    assert_eq!(approval.reason.as_deref(), Some("never_delegate"));
    assert_eq!(approval.decider_kind, None);
    assert!(approval.audit.is_empty(), "nothing was decided yet");
}

#[test]
fn a_destructive_reset_is_denied_by_the_default_rule() {
    let desk = desk("deny");
    link_project(&desk);
    let approval = desk.in_project("git reset --hard HEAD~1");

    assert_eq!(approval.status, ApprovalStatus::Denied);
    assert_eq!(approval.decider_id.as_deref(), Some("git-destructive"));
    assert_eq!(approval.audit[0].decision, "denied");
}

#[test]
fn an_action_no_rule_covers_waits_for_the_lead() {
    let desk = desk("lead");
    link_project(&desk);
    let approval = desk.in_project("npm install left-pad");

    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.level.as_deref(), Some("lead"));
    assert_eq!(approval.reason.as_deref(), Some("no_rule"));
}

#[test]
fn a_caller_asking_for_high_risk_is_never_allowed_by_the_policy() {
    let desk = desk("high");
    link_project(&desk);
    let approval = desk.ask("cargo test", &desk.root, Some(Risk::High));

    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.risk, Risk::High);
    assert_eq!(approval.reason.as_deref(), Some("high_risk"));
}

#[test]
fn without_a_project_the_policy_places_nothing_and_allows_nothing() {
    let desk = desk("unplaced");
    let approval = desk.ask("cargo test", Path::new("/work"), None);

    // No project, so no lead can be placed: the approval skips to the CTO.
    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.level.as_deref(), Some("cto"));
    assert_eq!(approval.reason.as_deref(), Some("decider_unavailable"));
}

#[test]
fn an_action_outside_the_project_goes_to_the_user() {
    let desk = desk("outside");
    link_project(&desk);
    let outside = desk.dir.join("elsewhere");
    std::fs::create_dir_all(&outside).expect("outside folder");
    let approval = desk.ask("cargo test", &outside, None);

    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.level.as_deref(), Some("user"));
    assert_eq!(approval.reason.as_deref(), Some("never_delegate"));
}

#[test]
fn approvals_toml_replaces_the_defaults_and_a_broken_edit_keeps_the_last_good_one() {
    let desk = desk("file");
    link_project(&desk);
    let file = desk.dir.join("approvals.toml");
    std::fs::write(
        &file,
        r#"version = 1

[[rule]]
id = "builds-only"
decision = "allow"
category = ["build"]
cwd_under = "project"
"#,
    )
    .expect("write rules");

    let test_run = desk.in_project("cargo test -p hn-core-chat");
    assert_eq!(
        test_run.status,
        ApprovalStatus::Pending,
        "the file replaces the defaults"
    );

    let build = desk.in_project("cargo check");
    assert_eq!(build.status, ApprovalStatus::Approved);
    assert_eq!(build.decider_id.as_deref(), Some("builds-only"));

    std::thread::sleep(Duration::from_millis(30));
    std::fs::write(&file, "version = 1\n[[rule]\n").expect("write a broken file");

    let still = desk.in_project("cargo check --workspace");
    assert_eq!(still.status, ApprovalStatus::Approved);
    assert_eq!(still.decider_id.as_deref(), Some("builds-only"));
}
