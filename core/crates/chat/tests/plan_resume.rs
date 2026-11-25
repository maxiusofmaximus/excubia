//! A plan the agent asks to run on approval is handed to Alfred once, when the
//! operator approves it: one system turn in its conversation, no user message, and
//! never a second hand-over for the same plan. Plans that do not ask, and plans
//! that are rejected, changed, archived or already linked, never start.

mod common;

use std::sync::Barrier;

use common::{team, Team};
use excubia_chat::{
    ChatError, ConversationUpdate, NewPlan, Plan, PlanDecision, PlanStatus, PlanStep, SectionChange,
    Touch, Turn,
};

fn steps() -> Vec<PlanStep> {
    (0..4)
        .map(|index| PlanStep {
            title: format!("Paso {index}"),
            touches: vec![Touch::Files],
        })
        .collect()
}

fn propose_by(t: &Team, proposer: &str, run_on_approval: bool) -> Plan {
    t.store
        .propose_plan_with(
            NewPlan {
                conversation_id: t.conversation.clone(),
                proposer_agent_id: proposer.to_string(),
                steps: steps(),
            },
            run_on_approval,
        )
        .expect("plan")
}

fn approve(t: &Team, plan: &Plan) -> Result<Plan, ChatError> {
    t.store
        .decide_plan(&plan.id, PlanDecision::Approve, None, "device-1")
}

fn claim(t: &Team, plan: &Plan) -> Option<Turn> {
    t.store.claim_plan_resume(&plan.id).expect("claim")
}

#[test]
fn one_approval_hands_one_system_turn_to_alfred_and_a_repeat_adds_none() {
    let t = team("resume-one");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");

    let turn = claim(&t, &plan).expect("the approved plan is handed over");
    assert_eq!(turn.agent_id, t.alfred);
    assert_eq!(turn.conversation_id, t.conversation);
    assert!(turn.message_id.is_empty(), "no user message is the anchor");
    assert!(turn.text.contains(&plan.id), "{}", turn.text);
    assert!(turn.text.contains("delegation.assign"), "{}", turn.text);
    assert!(
        turn.text.contains(&format!("to_agent_id \"{}\"", t.lead)),
        "the proposing lead is the destination: {}",
        turn.text
    );

    assert!(claim(&t, &plan).is_none(), "a second claim adds no turn");
    let again = approve(&t, &plan).expect("a repeated approval is the same answer");
    assert_eq!(again.status, PlanStatus::Approved);
    assert!(claim(&t, &plan).is_none(), "a repeat adds no turn either");
    assert!(t.store.pending_plan_resumes().expect("pending").is_empty());
}

#[test]
fn a_plan_proposed_by_alfred_goes_to_the_project_lead() {
    let t = team("resume-alfred");
    let plan = propose_by(&t, &t.alfred, true);
    approve(&t, &plan).expect("approve");
    let turn = claim(&t, &plan).expect("handed over");
    assert_eq!(turn.agent_id, t.alfred);
    assert!(
        turn.text.contains(&format!("to_agent_id \"{}\"", t.lead)),
        "{}",
        turn.text
    );
}

#[test]
fn a_plan_not_asked_to_run_is_never_handed_over() {
    let t = team("resume-plain");
    let plan = propose_by(&t, &t.lead, false);
    approve(&t, &plan).expect("approve");
    assert!(claim(&t, &plan).is_none());
    assert!(t.store.pending_plan_resumes().expect("pending").is_empty());
}

#[test]
fn a_rejected_or_changed_plan_never_starts() {
    let t = team("resume-refused");
    let rejected = propose_by(&t, &t.lead, true);
    t.store
        .decide_plan(&rejected.id, PlanDecision::Reject, None, "device-1")
        .expect("reject");
    assert!(claim(&t, &rejected).is_none());
    assert!(matches!(
        approve(&t, &rejected),
        Err(ChatError::PlanDecided)
    ));

    let changed = propose_by(&t, &t.lead, true);
    t.store
        .decide_plan(
            &changed.id,
            PlanDecision::RequestChanges,
            Some("Menos pasos".to_string()),
            "device-1",
        )
        .expect("request changes");
    assert!(claim(&t, &changed).is_none());
    assert!(matches!(approve(&t, &changed), Err(ChatError::PlanDecided)));
}

#[test]
fn a_refused_hand_over_stays_pending_until_it_is_queued_again() {
    let t = team("resume-retry");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");

    let first = claim(&t, &plan).expect("first hand-over");
    // The seat refused the turn: the plan is pending again and gets one new run.
    t.store.release_plan_resume(&plan.id).expect("release");
    assert_eq!(
        t.store.pending_plan_resumes().expect("pending"),
        vec![plan.id.clone()]
    );
    let second = claim(&t, &plan).expect("the retry hands it over");
    assert_ne!(first.id, second.id);
    assert!(claim(&t, &plan).is_none());
}

#[test]
fn an_archived_conversation_closes_its_resumption_without_a_turn() {
    let t = team("resume-archived");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");
    t.store
        .update_conversation(
            &t.conversation,
            ConversationUpdate {
                title: None,
                pinned: None,
                section: SectionChange::Keep,
                archived: Some(true),
            },
        )
        .expect("archive");
    assert!(claim(&t, &plan).is_none());
    assert!(t.store.pending_plan_resumes().expect("pending").is_empty());
}

#[test]
fn a_plan_a_delegation_already_started_is_not_run_again() {
    let t = team("resume-linked");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");
    let parent = t
        .assign(&t.alfred, &t.lead, "Equipo", "run_root", None)
        .expect("the lead's task");
    t.store
        .create_delegation(excubia_chat::NewDelegation {
            conversation_id: t.conversation.clone(),
            from_agent_id: t.lead.clone(),
            to_agent_id: t.specialist.clone(),
            task: "Montar la primera parte".to_string(),
            turn_key: parent.id.clone(),
            parent_id: Some(parent.id.clone()),
            steps: 4,
            plan_id: Some(plan.id.clone()),
        })
        .expect("the delegation takes the plan");
    assert!(claim(&t, &plan).is_none(), "the plan already started");
}

#[test]
fn a_plan_the_rule_approves_is_not_handed_over_again() {
    let t = team("resume-rule");
    std::fs::write(
        t.dir.join("approvals.toml"),
        "version = 1\n[plans]\nauto_approve_safe = true\n",
    )
    .expect("policy");
    let plan = propose_by(&t, &t.lead, true);
    assert_eq!(plan.status, PlanStatus::Approved, "the rule approved it");
    assert!(claim(&t, &plan).is_none(), "its agent already reads it as approved");
}

#[test]
fn concurrent_hand_overs_queue_exactly_one_turn() {
    let t = team("resume-race");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");

    let barrier = Barrier::new(4);
    let turns: Vec<Option<Turn>> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    t.store.claim_plan_resume(&plan.id).expect("claim")
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("worker"))
            .collect()
    });
    let queued = turns.iter().filter(|turn| turn.is_some()).count();
    assert_eq!(queued, 1, "one hand-over among four answers");
}

fn run_state(db: &std::path::Path, run_id: &str) -> (String, Option<String>) {
    let connection = rusqlite::Connection::open(db).expect("open the database");
    connection
        .query_row(
            "SELECT state, reason FROM run WHERE id = ?1",
            [run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("the run row")
}

#[test]
fn a_refused_hand_over_closes_its_reserved_run_as_failed() {
    let t = team("resume-close-run");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");
    let reserved = claim(&t, &plan).expect("reserved");
    assert_eq!(run_state(&t.db, &reserved.id).0, "queued");

    t.store.release_plan_resume(&plan.id).expect("release");
    assert_eq!(
        run_state(&t.db, &reserved.id),
        ("failed".to_string(), Some("seat_unavailable".to_string())),
        "the reservation never queued: it must not stay queued"
    );
    // A second release has nothing reserved and changes nothing.
    t.store.release_plan_resume(&plan.id).expect("second release");
    assert_eq!(run_state(&t.db, &reserved.id).0, "failed");
}

#[test]
fn a_restart_hands_over_a_reservation_the_seat_never_dequeued() {
    let mut t = team("resume-restart-pending");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");
    let reserved = claim(&t, &plan).expect("reserved, never dequeued");

    let db = t.db.clone();
    drop(t.store);
    let reopened = excubia_chat::ChatStore::open(&db).expect("restart");
    assert_eq!(
        reopened.pending_plan_resumes().expect("pending"),
        vec![plan.id.clone()],
        "the run that never ran puts the plan back to pending"
    );
    assert_eq!(run_state(&db, &reserved.id).0, "interrupted");
    let again = reopened
        .claim_plan_resume(&plan.id)
        .expect("claim")
        .expect("handed over once more");
    assert_ne!(again.id, reserved.id);
    t.store = reopened;
}

#[test]
fn a_restart_never_hands_over_again_a_turn_the_seat_already_dequeued() {
    let mut t = team("resume-restart-running");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");
    let reserved = claim(&t, &plan).expect("reserved");
    {
        let connection = rusqlite::Connection::open(&t.db).expect("open");
        connection
            .execute(
                "UPDATE run SET state = 'running' WHERE id = ?1",
                [&reserved.id],
            )
            .expect("the seat dequeued it");
    }

    let db = t.db.clone();
    drop(t.store);
    let reopened = excubia_chat::ChatStore::open(&db).expect("restart");
    assert!(reopened.pending_plan_resumes().expect("pending").is_empty());
    assert!(reopened.claim_plan_resume(&plan.id).expect("claim").is_none());
    t.store = reopened;
}

#[test]
fn a_recipe_run_never_takes_a_plan_the_approval_hands_to_alfred() {
    let t = team("resume-recipe");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");
    let refused = t.store.create_user_delegation(excubia_chat::NewDelegation {
        conversation_id: t.conversation.clone(),
        from_agent_id: t.alfred.clone(),
        to_agent_id: t.lead.clone(),
        task: "Receta de prueba".to_string(),
        turn_key: "run_recipe".to_string(),
        parent_id: None,
        steps: 4,
        plan_id: Some(plan.id.clone()),
    });
    assert!(
        matches!(refused, Err(ChatError::PlanLinked)),
        "a recipe must not start it: {refused:?}"
    );
    assert_eq!(
        claim(&t, &plan).map(|turn| turn.agent_id),
        Some(t.alfred.clone()),
        "the approval's own hand-over is still there"
    );
}

#[test]
fn a_plan_run_resolves_its_conversation_and_completes_without_a_user_message() {
    let t = team("resume-complete");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");
    let turn = claim(&t, &plan).expect("handed over");

    let message = t
        .store
        .complete_runs_with_files(&[turn.id.as_str()], "Encargo enviado.", &[], &[])
        .expect("the plan run completes");
    assert_eq!(message.conversation_id, t.conversation);
    assert_eq!(message.author_kind, "agent");
    assert_eq!(message.author_agent_id.as_deref(), Some(t.alfred.as_str()));
    assert_eq!(
        t.store.run_state(&turn.id).expect("state"),
        Some(excubia_chat::RunState::Completed)
    );
}

#[test]
fn stopping_the_conversation_stops_its_queued_plan_run() {
    let t = team("resume-stop");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");
    let turn = claim(&t, &plan).expect("handed over");

    let stopped = t
        .store
        .stop_conversation_runs(&t.conversation, "conversation_stopped")
        .expect("stop");
    assert_eq!(stopped, vec![turn.id.clone()]);
    assert_eq!(
        t.store.run_state(&turn.id).expect("state"),
        Some(excubia_chat::RunState::Interrupted)
    );
}

fn open_runs(t: &Team) -> i64 {
    rusqlite::Connection::open(&t.db)
        .expect("open the database")
        .query_row(
            "SELECT COUNT(*) FROM run WHERE state IN ('queued', 'running')",
            [],
            |row| row.get(0),
        )
        .expect("count")
}

fn run_row_exists(t: &Team, run_id: &str) -> bool {
    rusqlite::Connection::open(&t.db)
        .expect("open the database")
        .query_row("SELECT COUNT(*) FROM run WHERE id = ?1", [run_id], |row| {
            row.get::<_, i64>(0)
        })
        .expect("count")
        == 1
}

#[test]
fn archiving_a_conversation_closes_its_queued_plan_run_and_starts_nothing() {
    let t = team("resume-archive-queued");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");
    let turn = claim(&t, &plan).expect("handed over");

    t.store
        .update_conversation(
            &t.conversation,
            ConversationUpdate {
                title: None,
                pinned: None,
                section: SectionChange::Keep,
                archived: Some(true),
            },
        )
        .expect("archive");
    assert_eq!(t.store.run_state(&turn.id).expect("state"), Some(excubia_chat::RunState::Interrupted));
    assert_eq!(open_runs(&t), 0, "no run is left open");
    assert_eq!(t.store.start_run(&turn.id).expect("dequeue"), false, "the seat starts nothing");
}

#[test]
fn stopping_a_conversation_closes_its_queued_plan_run_and_starts_nothing() {
    let t = team("resume-stop-queued");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");
    let turn = claim(&t, &plan).expect("handed over");

    t.store
        .stop_conversation_runs(&t.conversation, "stopped_by_user")
        .expect("stop");
    assert_eq!(open_runs(&t), 0, "no run is left open");
    assert_eq!(t.store.start_run(&turn.id).expect("dequeue"), false, "the seat starts nothing");
    assert!(claim(&t, &plan).is_none(), "a stopped resumption is not handed over again");
}

#[test]
fn a_conversation_with_an_open_plan_run_is_busy_until_stopped_and_then_leaves_no_run_behind() {
    let t = team("resume-delete-queued");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");
    let turn = claim(&t, &plan).expect("handed over");

    assert!(
        matches!(
            t.store.delete_conversation(&t.conversation, "device-1"),
            Err(ChatError::ConversationBusy)
        ),
        "an open plan run keeps the conversation"
    );
    assert!(run_row_exists(&t, &turn.id));

    t.store
        .stop_conversation_runs(&t.conversation, "stopped_by_user")
        .expect("stop");
    t.store
        .delete_conversation(&t.conversation, "device-1")
        .expect("delete once stopped");
    assert!(!run_row_exists(&t, &turn.id), "no orphan run row remains");
    assert_eq!(open_runs(&t), 0);
}

#[test]
fn deleting_a_conversation_removes_its_finished_plan_run() {
    let t = team("resume-delete-finished");
    let plan = propose_by(&t, &t.lead, true);
    approve(&t, &plan).expect("approve");
    let turn = claim(&t, &plan).expect("handed over");
    t.store
        .complete_runs_with_files(&[turn.id.as_str()], "Encargo enviado.", &[], &[])
        .expect("complete");

    t.store
        .delete_conversation(&t.conversation, "device-1")
        .expect("delete");
    assert!(!run_row_exists(&t, &turn.id), "the finished run row goes with its conversation");
}
