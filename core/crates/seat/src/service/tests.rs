use std::future::Future;
use std::time::{Duration, Instant};

use excubia_chat::{
    ApprovalKind, ChatStore, Decision, HookScope, HookTokenUse, NewApproval, Risk, RunState, Turn,
};

use super::grant::HookTokenGuard;
use super::turn::{wait_for_turn, Outcome};
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
    let grant = RunGrant::new(&turn, vec!["run_1".to_string()]);
    *lock(&grant.phase) = Phase::Closed;
    let error = SeatService::check_phase(&lock(&grant.phase)).expect_err("closed");
    assert_eq!(error.code, "run_closed");
}

// The turn's timeout counts the CLI's active time. The waits below use real
// time with short budgets (a few hundred ms) and TTLs of one second.

fn grant_of(run_ids: &[&str]) -> RunGrant {
    let turn = Turn {
        id: run_ids[0].to_string(),
        state: RunState::Running,
        agent_id: "ag_1".to_string(),
        conversation_id: "cnv_1".to_string(),
        message_id: "msg_1".to_string(),
        text: "hola".to_string(),
    };
    RunGrant::new(&turn, run_ids.iter().map(|id| id.to_string()).collect())
}

/// A CLI that never exits on its own.
fn hung() -> impl Future<Output = Result<Option<i32>, String>> {
    std::future::pending()
}

/// A pending approval of `run_id`, as the hook or the MCP tool would open it.
fn ask(chat: &ChatStore, run_id: &str, digest: &str, ttl_seconds: u32) -> String {
    chat.create_approval(NewApproval {
        conversation_id: None,
        seat_id: None,
        agent_label: None,
        run_id: Some(run_id.to_string()),
        pane_id: None,
        kind: ApprovalKind::Command,
        tool: "bash".to_string(),
        summary: "cargo test".to_string(),
        cwd: None,
        paths: Vec::new(),
        risk: Some(Risk::High),
        action_digest: digest.to_string(),
        payload_redacted: None,
        ttl_seconds,
    })
    .expect("approval")
    .id
}

// The card is shown as soon as it exists: its TTL starts there, as it does for the operator.
fn ask_shown(chat: &ChatStore, run_id: &str, digest: &str, ttl_seconds: u32) -> String {
    let id = ask(chat, run_id, digest, ttl_seconds);
    chat.show_approval(&id).expect("shown");
    id
}

#[tokio::test]
async fn a_hung_cli_without_approvals_ends_at_the_budget() {
    let chat = ChatStore::in_memory().expect("store");
    let grant = grant_of(&["run_1"]);
    let started = Instant::now();
    let outcome = wait_for_turn(&grant, &chat, hung(), Duration::from_millis(200)).await;
    assert!(matches!(outcome, Outcome::Timeout));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn an_approval_longer_than_the_budget_resumes_the_turn_at_its_decision() {
    let chat = ChatStore::in_memory().expect("store");
    let grant = grant_of(&["run_1"]);
    let approval = ask(&chat, "run_1", "sha256:a", 30);
    let decide = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        chat.decide_approval(&approval, Decision::Approve, "device-1", "device-1")
            .expect("approved");
    };
    let started = Instant::now();
    let (outcome, ()) = tokio::join!(
        wait_for_turn(&grant, &chat, hung(), Duration::from_millis(300)),
        decide
    );
    assert!(matches!(outcome, Outcome::Timeout));
    // 300 ms of budget are left after the decision at 500 ms: the turn ends near
    // 800 ms. A budget that kept running would have ended it at 300 ms.
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(750), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(1500), "{elapsed:?}");
}

#[tokio::test]
async fn an_expired_approval_resumes_the_turn_with_the_balance_it_had() {
    let chat = ChatStore::in_memory().expect("store");
    let grant = grant_of(&["run_1"]);
    let later = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        ask_shown(&chat, "run_1", "sha256:a", 1);
    };
    let started = Instant::now();
    let (outcome, ()) = tokio::join!(
        wait_for_turn(&grant, &chat, hung(), Duration::from_millis(400)),
        later
    );
    assert!(matches!(outcome, Outcome::Timeout));
    // 300 ms ran before the card; it expires near 1.3 s with 100 ms of budget
    // left, so the turn ends near 1.4 s. A fresh 400 ms would end it near 1.7 s.
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(1350), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(1650), "{elapsed:?}");
}

#[tokio::test]
async fn overlapping_approvals_discount_their_union_once() {
    let chat = ChatStore::in_memory().expect("store");
    let grant = grant_of(&["run_1"]);
    let later = async {
        ask_shown(&chat, "run_1", "sha256:a", 1);
        tokio::time::sleep(Duration::from_millis(300)).await;
        ask_shown(&chat, "run_1", "sha256:b", 1);
    };
    let started = Instant::now();
    let (outcome, ()) = tokio::join!(
        wait_for_turn(&grant, &chat, hung(), Duration::from_millis(200)),
        later
    );
    assert!(matches!(outcome, Outcome::Timeout));
    // The union runs from 0 to the second card's expiry, near 1.3 s; 200 ms are
    // left, so the turn ends near 1.5 s. Discounting each card would end it sooner.
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(1450), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(1800), "{elapsed:?}");
}

#[tokio::test]
async fn sequential_approvals_that_add_up_past_the_budget_count_only_the_cli_time() {
    // The shape of the real failure: several cards one after another, each
    // waiting for the operator, whose waits add up past the budget. The CLI
    // works only 30 ms between them.
    let chat = ChatStore::in_memory().expect("store");
    let grant = grant_of(&["run_1"]);
    let cards = async {
        for digest in ["sha256:s1", "sha256:s2", "sha256:s3"] {
            let id = ask(&chat, "run_1", digest, 30);
            tokio::time::sleep(Duration::from_millis(200)).await;
            chat.decide_approval(&id, Decision::Approve, "device-1", "device-1")
                .expect("approved");
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    };
    let started = Instant::now();
    let (outcome, ()) = tokio::join!(
        wait_for_turn(&grant, &chat, hung(), Duration::from_millis(300)),
        cards
    );
    assert!(matches!(outcome, Outcome::Timeout));
    // The 600 ms of waits do not count. Only the 60 ms of CLI work between cards
    // does, so 240 ms of the 300 ms budget are left after the last card: the turn
    // ends near 0.9 s. Counting the waits would have ended it at 300 ms.
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(820), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(1150), "{elapsed:?}");
}

#[tokio::test]
async fn unrelated_frames_do_not_move_the_expiry_of_a_wait() {
    // A card of one second and a 400 ms budget. While it waits, twenty unrelated
    // frames arrive, one every 50 ms. None may push back the instant the card
    // expires, which was read once from the store.
    let chat = ChatStore::in_memory().expect("store");
    let grant = grant_of(&["run_1"]);
    let conversation = chat
        .create_conversation(excubia_chat::NewConversation {
            title: Some("Ruido".to_string()),
            project_id: None,
        })
        .expect("conversation")
        .id;
    let noise = async {
        ask_shown(&chat, "run_1", "sha256:a", 1);
        for i in 0..20 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            chat.append_user_message(&conversation, &format!("client-{i}"), "hola", None)
                .expect("message");
        }
    };
    let started = Instant::now();
    let (outcome, ()) = tokio::join!(
        wait_for_turn(&grant, &chat, hung(), Duration::from_millis(400)),
        noise
    );
    assert!(matches!(outcome, Outcome::Timeout));
    // The card expires near 1.0 s and 400 ms of budget are left: the turn ends
    // near 1.4 s. A wake point that slid with each unrelated frame ends near 2.4 s.
    let elapsed = started.elapsed();
    assert!(elapsed >= Duration::from_millis(1300), "{elapsed:?}");
    assert!(elapsed < Duration::from_millis(1700), "{elapsed:?}");
}

#[tokio::test]
async fn an_approval_of_another_run_does_not_suspend_the_turn() {
    let chat = ChatStore::in_memory().expect("store");
    let grant = grant_of(&["run_1"]);
    ask(&chat, "run_2", "sha256:a", 30);
    let started = Instant::now();
    let outcome = wait_for_turn(&grant, &chat, hung(), Duration::from_millis(200)).await;
    assert!(matches!(outcome, Outcome::Timeout));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn a_stop_during_the_wait_ends_the_turn_at_once() {
    let chat = ChatStore::in_memory().expect("store");
    let grant = grant_of(&["run_1"]);
    let stop = async {
        ask(&chat, "run_1", "sha256:a", 30);
        tokio::time::sleep(Duration::from_millis(50)).await;
        grant.stop.notify_one();
    };
    let (outcome, ()) = tokio::join!(
        wait_for_turn(&grant, &chat, hung(), Duration::from_secs(10)),
        stop
    );
    assert!(matches!(outcome, Outcome::Stopped));
}

#[tokio::test]
async fn a_reply_during_the_wait_is_the_answer_of_the_turn() {
    let chat = ChatStore::in_memory().expect("store");
    let grant = grant_of(&["run_1"]);
    let reply = async {
        ask(&chat, "run_1", "sha256:a", 30);
        tokio::time::sleep(Duration::from_millis(50)).await;
        grant.notify.notify_one();
    };
    let (outcome, ()) = tokio::join!(
        wait_for_turn(&grant, &chat, hung(), Duration::from_secs(10)),
        reply
    );
    assert!(matches!(outcome, Outcome::Replied));
}

#[tokio::test]
async fn a_process_that_exits_during_the_wait_ends_the_turn_at_once() {
    let chat = ChatStore::in_memory().expect("store");
    let grant = grant_of(&["run_1"]);
    ask(&chat, "run_1", "sha256:a", 30);
    let exit = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok(Some(3))
    };
    let outcome = wait_for_turn(&grant, &chat, exit, Duration::from_secs(10)).await;
    assert!(matches!(outcome, Outcome::Exited(Some(3))));
}

#[test]
fn an_approved_action_is_burned_once() {
    let chat = ChatStore::in_memory().expect("store");
    let approval = ask(&chat, "run_1", "sha256:a", 30);
    chat.decide_approval(&approval, Decision::Approve, "device-1", "device-1")
        .expect("approved");
    chat.consume_for_host(&approval, "sha256:a")
        .expect("the first burn runs the action");
    assert!(
        chat.consume_for_host(&approval, "sha256:a").is_err(),
        "a second burn is refused, never a second execution"
    );
}
