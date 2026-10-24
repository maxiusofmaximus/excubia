//! The approval mode (CORE9-D, RFC §5): `always_user` sends the trivial
//! approvals to the operator too, a project overrides the global mode, a
//! replayed change changes nothing, and a change climbs the approvals it sends
//! up. Every accepted change is audited and published.

mod common;

use common::{team, Team};
use hn_core_chat::{
    action_digest, Approval, ApprovalKind, ApprovalMode, ApprovalStatus, ChatError, NewApproval,
    DEFAULT_TTL_SECONDS,
};
use serde_json::json;

/// A command asked by the specialist from the project root.
fn ask(team: &Team, command: &str) -> Approval {
    let args = json!({ "command": command });
    let cwd = team.root.to_string_lossy().to_string();
    team.store
        .create_approval(NewApproval {
            conversation_id: Some(team.conversation.clone()),
            seat_id: Some(team.specialist.clone()),
            agent_label: Some("ana".to_string()),
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: format!("Ejecutar comando: {command}"),
            cwd: Some(cwd.clone()),
            paths: Vec::new(),
            risk: None,
            action_digest: action_digest("command", "bash", &args, &cwd, &[]),
            payload_redacted: Some(args),
            ttl_seconds: DEFAULT_TTL_SECONDS,
        })
        .expect("approval")
}

/// The audit rows of the approval mode: the scope, the modes and the device.
fn settings_audit(team: &Team) -> Vec<(String, String, String, String)> {
    let connection = rusqlite::Connection::open(&team.db).expect("raw database");
    let mut statement = connection
        .prepare(
            "SELECT scope, from_mode, to_mode, decider_id
               FROM approval_settings_audit ORDER BY id",
        )
        .expect("audit query");
    statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .expect("audit rows")
        .map(|row| row.expect("audit row"))
        .collect()
}

/// The idempotency key and the decider of each accepted change, in order.
fn settings_audit_keys(team: &Team) -> Vec<(String, String)> {
    let connection = rusqlite::Connection::open(&team.db).expect("raw database");
    let mut statement = connection
        .prepare("SELECT client_id, decider_id FROM approval_settings_audit ORDER BY id")
        .expect("audit query");
    statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("audit rows")
        .map(|row| row.expect("audit row"))
        .collect()
}

#[test]
fn always_user_sends_a_trivial_command_to_the_operator() {
    let team = team("mode-global");
    let before = ask(&team, "cargo test");
    assert_eq!(
        before.status,
        ApprovalStatus::Approved,
        "the policy allows tests"
    );

    team.store
        .set_approval_mode(
            "req-global",
            "device-global",
            None,
            ApprovalMode::AlwaysUser,
        )
        .expect("set the mode");
    let after = ask(&team, "cargo test --workspace");
    assert_eq!(after.status, ApprovalStatus::Pending);
    assert_eq!(after.level.as_deref(), Some("user"));
    assert_eq!(after.escalated_from.as_deref(), Some("policy"));
    assert_eq!(after.reason.as_deref(), Some("always_user"));
}

#[test]
fn a_project_mode_overrides_the_global_one_both_ways() {
    let team = team("mode-project");
    team.store
        .set_approval_mode(
            "req-global",
            "device-global",
            None,
            ApprovalMode::AlwaysUser,
        )
        .expect("global");
    team.store
        .set_approval_mode(
            "req-project",
            "device-project",
            Some(&team.project),
            ApprovalMode::Delegated,
        )
        .expect("project");

    // The project says delegated, so the policy still allows its tests.
    let allowed = ask(&team, "cargo test");
    assert_eq!(allowed.status, ApprovalStatus::Approved);

    // Without the override the global operator mode would send it up.
    let settings = team.store.approval_settings().expect("settings");
    assert_eq!(settings.global, ApprovalMode::AlwaysUser);
    assert_eq!(settings.projects.len(), 1);
    assert_eq!(settings.projects[0].project_id, team.project);
    assert_eq!(settings.projects[0].mode, ApprovalMode::Delegated);
}

#[test]
fn a_replayed_change_changes_nothing_and_is_audited_once() {
    let team = team("mode-replay");
    let first = team
        .store
        .set_approval_mode("req-1", "device-1", None, ApprovalMode::AlwaysUser)
        .expect("first");
    assert!(!first.replayed);

    // The same client_id again, with another mode: the first answer stands.
    let replay = team
        .store
        .set_approval_mode("req-1", "device-1", None, ApprovalMode::Delegated)
        .expect("replay");
    assert!(replay.replayed);
    assert_eq!(replay.settings.global, ApprovalMode::AlwaysUser);
    assert_eq!(
        settings_audit(&team),
        vec![(
            "global".to_string(),
            "delegated".to_string(),
            "always_user".to_string(),
            "device-1".to_string(),
        )]
    );
}

#[test]
fn a_change_for_an_unknown_project_is_refused_and_leaves_no_trace() {
    let team = team("mode-unknown");
    let refused = team.store.set_approval_mode(
        "req-2",
        "device-2",
        Some("prj_missing"),
        ApprovalMode::AlwaysUser,
    );
    assert!(matches!(refused, Err(ChatError::UnknownProject(_))));
    assert!(settings_audit(&team).is_empty());
}

#[test]
fn switching_to_always_user_climbs_the_waiting_approvals_to_the_operator() {
    let team = team("mode-climb");
    let waiting = ask(&team, "npm install left-pad");
    assert_eq!(waiting.level.as_deref(), Some("lead"));
    assert_eq!(
        team.store
            .summary()
            .expect("summary")
            .pending_user_approvals,
        0
    );

    let change = team
        .store
        .set_approval_mode("req-3", "device-3", None, ApprovalMode::AlwaysUser)
        .expect("switch");
    assert_eq!(change.escalated, 1);

    let climbed = team.store.approval(&waiting.id).expect("approval");
    assert_eq!(climbed.status, ApprovalStatus::Pending, "it is not lost");
    assert_eq!(climbed.level.as_deref(), Some("user"));
    assert_eq!(climbed.escalated_from.as_deref(), Some("lead"));
    assert_eq!(climbed.reason.as_deref(), Some("mode_changed"));
    assert_eq!(
        team.store
            .summary()
            .expect("summary")
            .pending_user_approvals,
        1
    );

    let connection = rusqlite::Connection::open(&team.db).expect("raw database");
    let (decider_kind, decider_id, level, decision): (String, String, String, String) = connection
        .query_row(
            "SELECT decider_kind, decider_id, level, decision FROM approval_audit
              WHERE approval_id = ?1 ORDER BY id DESC LIMIT 1",
            [&waiting.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("the climb is audited");
    assert_eq!(
        (
            decider_kind.as_str(),
            decider_id.as_str(),
            level.as_str(),
            decision.as_str()
        ),
        ("user", "device-3", "user", "escalated")
    );
}

#[test]
fn a_change_is_published_as_a_settings_frame() {
    let team = team("mode-frame");
    let mut events = team.store.subscribe();
    team.store
        .set_approval_mode("req-4", "device-4", None, ApprovalMode::AlwaysUser)
        .expect("switch");

    let frame = std::iter::from_fn(|| events.try_recv().ok())
        .find(|frame| frame.event == "chat.approval_settings")
        .expect("the change is published");
    assert_eq!(frame.body["scope"], "global");
    assert_eq!(frame.body["mode"], "always_user");
    assert_eq!(frame.body["previous"], "delegated");
    assert!(frame.id > 0, "the frame is journaled with a revision");
}

#[test]
fn the_audit_names_the_signing_device_and_keeps_the_client_id_as_the_key() {
    let team = team("mode-device");
    team.store
        .set_approval_mode("req-7", "device-7", None, ApprovalMode::AlwaysUser)
        .expect("switch");

    assert_eq!(
        settings_audit_keys(&team),
        vec![("req-7".to_string(), "device-7".to_string())],
        "(client_id, decider_id): the key is the request, the decider the device"
    );
    let stored = team.store.approval_settings().expect("settings");
    assert_eq!(stored.global, ApprovalMode::AlwaysUser);
}
