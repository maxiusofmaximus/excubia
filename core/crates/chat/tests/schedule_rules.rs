//! F4-CORE store rules, the second half: the plan a schedule needs, the brakes on its count, the
//! history it keeps, its recipe and its text, the conversation it lives in, the version it migrates
//! from, and the clock change it keeps its nine o'clock across.

mod common;
mod schedule_support;

use common::team;
use hn_core_chat::{
    ChatError, ChatStore, Content, HireFilter, NewConversation, NewHire, RecipeText, HISTORY_KEEP,
    MAX_ACTIVE_SCHEDULES,
};

use schedule_support::*;

#[test]
fn more_than_a_plan_threshold_needs_an_approved_plan_held_by_one_live_schedule() {
    let t = team("sched-plan");
    hire(&t);
    set_clock(&t, MON_0700);
    let mut without = body(&t, "Larga", "Pasos.");
    without.steps = 4;
    assert!(matches!(
        t.store.create_schedule(without),
        Err(ChatError::PlanRequired)
    ));

    let approved = plan(&t, 4);
    let mut with = body(&t, "Larga", "Pasos.");
    with.steps = 4;
    with.plan_id = Some(approved.clone());
    let first = t.store.create_schedule(with.clone()).expect("first");
    assert_eq!(first.plan_id.as_deref(), Some(approved.as_str()));

    let mut second = body(&t, "Otra", "Pasos.");
    second.steps = 4;
    second.plan_id = Some(approved.clone());
    assert!(matches!(
        t.store.create_schedule(second),
        Err(ChatError::PlanLinked)
    ));
}

#[test]
fn an_edited_text_expires_its_plan_and_a_new_plan_must_be_approved() {
    let t = team("sched-expire");
    hire(&t);
    set_clock(&t, MON_0700);
    let approved = plan(&t, 4);
    let mut new = body(&t, "Larga", "Pasos.");
    new.steps = 4;
    new.plan_id = Some(approved.clone());
    let schedule = t.store.create_schedule(new).expect("create");

    let mut same_plan = body(&t, "Larga", "Pasos cambiados.");
    same_plan.steps = 4;
    same_plan.plan_id = Some(approved.clone());
    assert!(matches!(
        t.store.update_schedule(&schedule.id, same_plan),
        Err(ChatError::PlanExpired)
    ));

    let mut without = body(&t, "Larga", "Pasos cambiados.");
    without.steps = 4;
    assert!(matches!(
        t.store.update_schedule(&schedule.id, without),
        Err(ChatError::PlanRequired)
    ));
}

#[test]
fn the_company_keeps_at_most_twenty_active_schedules() {
    let t = team("sched-limit");
    hire(&t);
    set_clock(&t, MON_0700);
    for n in 0..MAX_ACTIVE_SCHEDULES {
        t.store
            .create_schedule(body(&t, &format!("Tarea {n}"), "Hazlo."))
            .expect("within the limit");
    }
    assert!(matches!(
        t.store.create_schedule(body(&t, "Una de más", "Hazlo.")),
        Err(ChatError::ScheduleLimit)
    ));
}

#[test]
fn a_recipe_schedule_runs_the_text_of_the_recipe() {
    let t = team("sched-recipe");
    hire(&t);
    set_clock(&t, MON_0700);
    let recipe = t
        .store
        .create_recipe(
            ROLE,
            hn_core_chat::RecipeText {
                title: "Plantilla".to_string(),
                body: "Resume la semana.".to_string(),
            },
            false,
        )
        .expect("recipe");
    let mut new = body(&t, "Desde receta", "");
    new.content = Content::Recipe(recipe.id.clone());
    t.store.create_schedule(new).expect("create");
    set_clock(&t, "2026-11-02T08:00:05Z");
    let launch = t.store.claim_due_schedules().expect("claim").remove(0);
    assert_eq!(launch.run.task, "Resume la semana.");
    assert_eq!(launch.run.recipe_id.as_deref(), Some(recipe.id.as_str()));
}

#[test]
fn a_text_with_a_secret_is_never_kept() {
    let t = team("sched-secret");
    set_clock(&t, MON_0700);
    let refused = t.store.create_schedule(body(
        &t,
        "Con clave",
        "Usa esta clave: sk-proj-abcdefghijklmnopqrstuvwx123456 para la llamada.",
    ));
    assert!(matches!(refused, Err(ChatError::RecipeSecret(_))));
}

#[test]
fn the_history_keeps_its_newest_fifty_runs() {
    let t = team("sched-history");
    hire(&t);
    set_clock(&t, MON_0700);
    let schedule = t
        .store
        .create_schedule(body(&t, "Resumen", "Haz un resumen."))
        .expect("create");
    set_clock(&t, "2026-11-02T08:01:00Z");
    for _ in 0..(HISTORY_KEEP + 5) {
        t.store.claim_schedule_now(&schedule.id).expect("run");
    }
    assert_eq!(
        t.store.schedule_runs(&schedule.id).expect("runs").len(),
        HISTORY_KEEP as usize
    );
}

#[test]
fn a_schedule_needs_a_conversation_inside_a_project() {
    let t = team("sched-no-project");
    set_clock(&t, MON_0700);
    let loose = t
        .store
        .create_conversation(NewConversation {
            title: Some("Suelta".to_string()),
            project_id: None,
        })
        .expect("conversation");
    let mut new = body(&t, "Suelta", "Hazlo.");
    new.conversation_id = loose.id;
    assert!(matches!(
        t.store.create_schedule(new),
        Err(ChatError::ConversationWithoutProject)
    ));
}

#[test]
fn a_store_of_the_previous_version_keeps_its_data_and_gains_the_schedules() {
    let dir = common::temp_dir("schedule-migration");
    let db = dir.join("chat.db");
    {
        let store = ChatStore::open(&db).expect("open");
        store
            .hire(NewHire {
                catalog_role: ROLE.to_string(),
                area: "audio".to_string(),
                name: "Ana".to_string(),
                avatar: None,
                engine: None,
                profile: None,
                skills_allow: Vec::new(),
            })
            .expect("hire");
        store
            .create_recipe(
                ROLE,
                RecipeText {
                    title: "Plantilla".to_string(),
                    body: "Resume la semana.".to_string(),
                },
                false,
            )
            .expect("recipe");
    }
    {
        let raw = rusqlite::Connection::open(&db).expect("raw");
        raw.execute_batch(
            "DROP TABLE schedule_run; DROP TABLE schedule; PRAGMA user_version = 24;",
        )
        .expect("downgrade to version 24");
    }

    let store = ChatStore::open(&db).expect("reopen");
    assert_eq!(
        store.list_hires(HireFilter::Active).expect("hires").len(),
        1
    );
    assert_eq!(store.list_recipes(None).expect("recipes").len(), 1);
    assert!(store
        .schedules(None)
        .expect("the schedule table is back")
        .is_empty());
    let raw = rusqlite::Connection::open(&db).expect("raw");
    let version: i64 = raw
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, 25);
}

#[test]
fn a_daily_nine_oclock_keeps_nine_across_the_spring_clock_change() {
    let t = team("sched-dst");
    hire(&t);
    // Madrid is CET on 27 March 2027 and CEST from 28 March: 09:00 is 08:00Z, then 07:00Z.
    set_clock(&t, "2027-03-27T07:00:00Z");
    let mut new = body(&t, "Resumen", "Haz un resumen.");
    new.timing = timing(daily(), "2027-03-27");
    let schedule = t.store.create_schedule(new).expect("create");
    assert_eq!(
        schedule.next_run_at.as_deref(),
        Some("2027-03-27T08:00:00.000Z")
    );
    set_clock(&t, "2027-03-27T08:00:30Z");
    assert_eq!(t.store.claim_due_schedules().expect("claim").len(), 1);
    let after = t.store.schedule(&schedule.id).expect("schedule");
    assert_eq!(
        after.next_run_at.as_deref(),
        Some("2027-03-28T07:00:00.000Z")
    );
}
