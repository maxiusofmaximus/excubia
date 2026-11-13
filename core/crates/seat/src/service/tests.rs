use excubia_chat::{ChatStore, HookScope, HookTokenUse, RunState, Turn};

use super::grant::HookTokenGuard;
use super::{lock, Phase, RunGrant, SeatService};

#[test]
fn the_guard_revokes_the_token_of_the_run() {
    let chat = ChatStore::in_memory().expect("store");
    let token = chat
        .issue_hook_token(&HookScope::run("run_1", "cnv_1", "ag_1"))
        .expect("token");
    {
        let _guard = HookTokenGuard {
            chat: &chat,
            run_id: "run_1",
        };
    }
    assert_eq!(
        chat.hook_token_use(&token).expect("use"),
        HookTokenUse::Revoked
    );
}

#[test]
fn a_closed_turn_refuses_a_late_reply() {
    let turn = Turn {
        id: "run_1".to_string(),
        state: RunState::Running,
        agent_id: "ag_1".to_string(),
        conversation_id: "cnv_1".to_string(),
        message_id: "msg_1".to_string(),
        text: "hola".to_string(),
    };
    let grant = RunGrant::new(&turn);
    *lock(&grant.phase) = Phase::Closed;
    let error = SeatService::check_phase(&lock(&grant.phase)).expect_err("closed");
    assert_eq!(error.code, "run_closed");
}
