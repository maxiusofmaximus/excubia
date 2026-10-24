//! A slow scorer does not hold the database (CORE9-F2). The question is asked
//! before the write lock is taken, so a write from another request goes
//! through while the scorer is still answering.

mod common;
#[allow(dead_code)]
mod scorer_support;

use std::time::{Duration, Instant};

use common::{team, Team};
use hn_core_chat::{action_digest, ApprovalKind, ApprovalStatus, NewApproval, DEFAULT_TTL_SECONDS};
use scorer_support::{Behaviour, Scorer};
use serde_json::json;

/// Long enough that a write held behind the scorer would be measured, short
/// enough to stay inside the 200 ms budget: the answer counts.
const SCORER_ANSWERS_IN: Duration = Duration::from_millis(150);

/// A write that did not wait for the scorer finishes far below its answer.
const WRITE_WITHOUT_WAITING: Duration = Duration::from_millis(100);

fn command_approval(team: &Team, command: &str) -> NewApproval {
    let args = json!({ "command": command });
    let cwd = team.root.to_string_lossy().to_string();
    NewApproval {
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
    }
}

#[test]
fn a_slow_scorer_does_not_hold_the_database() {
    let scorer = Scorer::start(Behaviour::Sleeps(SCORER_ANSWERS_IN));
    let team = team("scorer-latency");
    let config = format!(
        "version = 1\n\n[scorer]\nbackend = \"local_http\"\nendpoint = \"http://127.0.0.1:{}\"\nauto_ok_if_low = [\"read\"]\n",
        scorer.port
    );
    std::fs::write(team.dir.join("approvals.toml"), config).expect("approvals.toml");

    std::thread::scope(|scope| {
        let asking = scope.spawn(|| {
            team.store
                .create_approval(command_approval(&team, "cat README.md"))
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while scorer.hits() == 0 {
            assert!(Instant::now() < deadline, "the scorer was never asked");
            std::thread::sleep(Duration::from_millis(2));
        }

        let started = Instant::now();
        let write = team
            .store
            .create_approval(command_approval(&team, "rm -rf target"))
            .expect("a write while the scorer answers");
        let waited = started.elapsed();
        assert!(
            waited < WRITE_WITHOUT_WAITING,
            "the write waited {waited:?} for the scorer"
        );
        assert_eq!(write.status, ApprovalStatus::Pending);

        let judged = asking
            .join()
            .expect("the scoring thread")
            .expect("the scored approval");
        assert_eq!(judged.status, ApprovalStatus::Approved);
        assert_eq!(judged.decider_kind.as_deref(), Some("scorer"));
    });
}
