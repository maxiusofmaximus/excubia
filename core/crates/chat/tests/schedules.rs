//! F4-CORE store rules: a schedule is a text (or a recipe) that runs at its clock. A due run
//! is launched when the host reaches it on time, missed and announced when it is late, and
//! skipped when the previous run is still going or a brake is reached. A one-time run is done
//! after it runs, and a plan above the step threshold is approved once for the live schedule.

mod common;
mod schedule_support;

use common::team;
use excubia_chat::{ChatError, Claim, RunOutcome, ScheduleStatus};

use schedule_support::*;

#[test]
fn a_new_daily_schedule_runs_at_its_next_local_nine() {
    let t = team("sched-next");
    hire(&t);
    set_clock(&t, MON_0700);
    let schedule = t
        .store
        .create_schedule(body(&t, "Resumen", "Haz un resumen."))
        .expect("create");
    assert_eq!(schedule.status, ScheduleStatus::Active);
    assert_eq!(
        schedule.next_run_at.as_deref(),
        Some("2026-11-02T08:00:00.000Z")
    );
}

#[test]
fn a_run_reached_on_time_is_launched_and_the_schedule_moves_to_tomorrow() {
    let t = team("sched-on-time");
    hire(&t);
    set_clock(&t, MON_0700);
    let schedule = t
        .store
        .create_schedule(body(&t, "Resumen", "Haz un resumen."))
        .expect("create");
    set_clock(&t, "2026-11-02T08:00:30Z");
    let launches = t.store.claim_due_schedules().expect("claim");
    assert_eq!(launches.len(), 1);
    assert_eq!(launches[0].run.task, "Haz un resumen.");
    assert_eq!(launches[0].run.recipe_id, None);
    let after = t.store.schedule(&schedule.id).expect("schedule");
    assert_eq!(
        after.next_run_at.as_deref(),
        Some("2026-11-03T08:00:00.000Z")
    );
    assert!(after.last_run_at.is_some());
    let runs = t.store.schedule_runs(&schedule.id).expect("runs");
    assert_eq!(runs[0].outcome, RunOutcome::Started);
}

#[test]
fn a_run_found_late_after_the_host_was_off_is_missed_and_announced_without_its_text() {
    let t = team("sched-late");
    hire(&t);
    set_clock(&t, MON_0700);
    let schedule = t
        .store
        .create_schedule(body(&t, "Resumen", "Texto secreto del encargo."))
        .expect("create");
    set_clock(&t, "2026-11-04T10:00:00Z");
    assert!(t.store.claim_due_schedules().expect("claim").is_empty());
    let runs = t.store.schedule_runs(&schedule.id).expect("runs");
    assert_eq!(runs[0].outcome, RunOutcome::MissedOffline);
    let page = t
        .store
        .messages(&t.conversation, None, None, 50)
        .expect("messages");
    let notice = page.messages.last().expect("the notice").body_md.clone();
    assert!(notice.contains("Resumen"));
    assert!(notice.contains("9:00"));
    assert!(!notice.contains("Texto secreto"));
}

#[test]
fn a_run_while_the_previous_one_is_still_going_is_skipped_as_overlap() {
    let t = team("sched-overlap");
    hire(&t);
    set_clock(&t, MON_0700);
    let schedule = t
        .store
        .create_schedule(body(&t, "Resumen", "Haz un resumen."))
        .expect("create");
    set_clock(&t, "2026-11-02T08:01:00Z");
    let first = t.store.claim_schedule_now(&schedule.id).expect("first");
    assert!(matches!(first, Claim::Launch(_)));
    match t.store.claim_schedule_now(&schedule.id).expect("second") {
        Claim::Recorded(run) => assert_eq!(run.outcome, RunOutcome::SkippedOverlap),
        other => panic!("expected a skipped run, got {other:?}"),
    }
}

#[test]
fn the_hourly_brake_per_oficio_skips_a_second_run_and_says_so() {
    let t = team("sched-cap");
    hire(&t);
    set_clock(&t, MON_0700);
    let first = t
        .store
        .create_schedule(body(&t, "Uno", "Primero."))
        .expect("create");
    let second = t
        .store
        .create_schedule(body(&t, "Dos", "Segundo."))
        .expect("create");
    set_clock(&t, "2026-11-02T08:01:00Z");
    let launch = launched(t.store.claim_schedule_now(&first.id).expect("first"));
    t.store
        .settle_schedule_run(&launch.run_id, Some("dlg_unknown"))
        .expect("settle");
    match t.store.claim_schedule_now(&second.id).expect("second") {
        Claim::Recorded(run) => assert_eq!(run.outcome, RunOutcome::SkippedCap),
        other => panic!("expected the brake to stop it, got {other:?}"),
    }
    let page = t
        .store
        .messages(&t.conversation, None, None, 50)
        .expect("messages");
    assert!(page
        .messages
        .last()
        .expect("notice")
        .body_md
        .contains("tope"));
}

#[test]
fn a_one_time_schedule_is_done_after_its_run() {
    let t = team("sched-once");
    hire(&t);
    set_clock(&t, MON_0700);
    let mut new = body(&t, "Una vez", "Hazlo una vez.");
    new.timing = timing(None, "2026-11-02");
    let schedule = t.store.create_schedule(new).expect("create");
    set_clock(&t, "2026-11-02T08:00:10Z");
    assert_eq!(t.store.claim_due_schedules().expect("claim").len(), 1);
    let after = t.store.schedule(&schedule.id).expect("schedule");
    assert_eq!(after.status, ScheduleStatus::Done);
    assert_eq!(after.next_run_at, None);
}

#[test]
fn a_one_time_schedule_whose_moment_has_passed_is_refused() {
    let t = team("sched-past");
    hire(&t);
    set_clock(&t, "2026-11-05T07:00:00Z");
    let mut new = body(&t, "Pasada", "Ya fue.");
    new.timing = timing(None, "2026-11-02");
    assert!(matches!(
        t.store.create_schedule(new),
        Err(ChatError::ScheduleNoFutureRun)
    ));
}

#[test]
fn a_paused_schedule_is_not_run_and_resumes_from_now_without_making_up_the_past() {
    let t = team("sched-pause");
    hire(&t);
    set_clock(&t, MON_0700);
    let schedule = t
        .store
        .create_schedule(body(&t, "Resumen", "Haz un resumen."))
        .expect("create");
    t.store.pause_schedule(&schedule.id).expect("pause");
    set_clock(&t, "2026-11-03T10:00:00Z");
    assert!(t.store.claim_due_schedules().expect("claim").is_empty());
    let resumed = t.store.resume_schedule(&schedule.id).expect("resume");
    assert_eq!(
        resumed.next_run_at.as_deref(),
        Some("2026-11-04T08:00:00.000Z")
    );
}

#[test]
fn a_done_schedule_cannot_be_resumed() {
    let t = team("sched-done");
    hire(&t);
    set_clock(&t, MON_0700);
    let mut new = body(&t, "Una vez", "Hazlo.");
    new.timing = timing(None, "2026-11-02");
    let schedule = t.store.create_schedule(new).expect("create");
    t.store.claim_schedule_now(&schedule.id).expect("run");
    assert!(matches!(
        t.store.resume_schedule(&schedule.id),
        Err(ChatError::ScheduleDone)
    ));
}
