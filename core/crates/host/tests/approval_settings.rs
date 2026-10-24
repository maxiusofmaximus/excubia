//! The approval mode over the real host (CORE9-D, RFC §5 and §8): only a device
//! that holds `APPROVE` changes it, an agent's own hook cannot reach the route,
//! and a switch to `always_user` moves the count the operator reads.

mod harness;

use harness::TestHost;
use hn_core_auth::Permission;
use hn_core_chat::{
    action_digest, ApprovalKind, NewApproval, NewConversation, DEFAULT_TTL_SECONDS,
};
use serde_json::{json, Value};

const SETTINGS: &str = "/v1/chat/approvals/settings";

fn body(client_id: &str, mode: &str) -> Option<Vec<u8>> {
    Some(
        json!({ "client_id": client_id, "mode": mode })
            .to_string()
            .into_bytes(),
    )
}

/// One command the host has no rule for, opened without a conversation.
fn ask(host: &TestHost, conversation_id: Option<&str>, command: &str) -> hn_core_chat::Approval {
    let args = json!({ "command": command });
    host.chat
        .create_approval(NewApproval {
            conversation_id: conversation_id.map(str::to_string),
            seat_id: None,
            agent_label: Some("ana".to_string()),
            run_id: None,
            pane_id: None,
            kind: ApprovalKind::Command,
            tool: "bash".to_string(),
            summary: format!("Ejecutar comando: {command}"),
            cwd: None,
            paths: Vec::new(),
            risk: None,
            action_digest: action_digest("command", "bash", &args, "", &[]),
            payload_redacted: Some(args),
            ttl_seconds: DEFAULT_TTL_SECONDS,
        })
        .expect("approval")
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_device_with_approve_changes_the_mode() {
    let host = TestHost::start("approval-mode-grant").await;
    let reader = host.paired_with("lector", &[Permission::ReadState]).await;
    let admin = host
        .paired_with("admin", &[Permission::Admin, Permission::ReadState])
        .await;
    let operator = host
        .paired_with("operador", &[Permission::Approve, Permission::ReadState])
        .await;

    // Reading is `READ_STATE`; changing is `APPROVE`. `ADMIN` does not replace it.
    let (status, _) = reader
        .request("POST", SETTINGS, body("lector-1", "always_user"))
        .await
        .expect("request");
    assert_eq!(status, 403, "a reader cannot change the mode");
    let (status, text) = admin
        .request("POST", SETTINGS, body("admin-1", "always_user"))
        .await
        .expect("request");
    assert_eq!(status, 403, "ADMIN is not APPROVE: {text}");

    let (status, text) = operator
        .request("POST", SETTINGS, body("op-1", "always_user"))
        .await
        .expect("request");
    assert_eq!(status, 200, "{text}");
    let answer: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(answer["settings"]["global"], "always_user");
    assert_eq!(answer["replayed"], false);

    let (status, text) = reader
        .request("GET", SETTINGS, None)
        .await
        .expect("request");
    assert_eq!(status, 200, "{text}");
    let read: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(read["settings"]["global"], "always_user");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_hook_cannot_reach_the_mode() {
    let host = TestHost::start("approval-mode-agent").await;
    let run = host.run_hook("mode");

    let (status, _) = host
        .hook_request(
            &run.token,
            "POST",
            SETTINGS,
            body("agente-1", "always_user"),
        )
        .await;
    assert_ne!(status, 200, "a hook token never changes the mode");
    let operator = host
        .paired_with("operador", &[Permission::Approve, Permission::ReadState])
        .await;
    let settings = operator
        .request("GET", SETTINGS, None)
        .await
        .expect("request");
    assert!(
        !settings.1.contains("always_user"),
        "the refused change left the mode as it was: {}",
        settings.1
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_switch_to_always_user_moves_the_count_the_operator_reads() {
    let host = TestHost::start("approval-mode-count").await;
    let operator = host
        .paired_with("operador", &[Permission::Approve, Permission::ReadState])
        .await;
    // A conversation keeps the approval below the operator until the mode says
    // otherwise. (Without one, the operator already holds it: see the inbox.)
    let conversation = host
        .chat
        .create_conversation(NewConversation {
            title: Some("Trabajo".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id;
    let waiting = ask(&host, Some(&conversation), "npm install left-pad");
    let before = operator.state().await.expect("state");
    assert_eq!(before["chat"]["pending_approvals"], 1);
    assert_eq!(before["chat"]["pending_user_approvals"], 0, "{before}");

    let (status, text) = operator
        .request("POST", SETTINGS, body("op-count", "always_user"))
        .await
        .expect("request");
    assert_eq!(status, 200, "{text}");

    let state = operator.state().await.expect("state");
    assert_eq!(state["chat"]["pending_user_approvals"], 1, "{state}");
    let climbed = host.chat.approval(&waiting.id).expect("approval");
    assert_eq!(climbed.level.as_deref(), Some("user"));
    assert_eq!(climbed.reason.as_deref(), Some("mode_changed"));
}
