//! Shared fixtures of the scheduled-work tests (F4-CORE): a team with an oficio hired, a clock
//! the tests move, and the schedules and plans they build on.

#![allow(dead_code)] // each test file uses its own part of these fixtures

use std::sync::Arc;

use crate::common::Team;
use chrono::{NaiveDate, NaiveTime};
use chrono_tz::Europe::Madrid;
use excubia_core_chat::{
    cadence_of, Claim, Content, ManualClock, NewHire, NewPlan, NewSchedule, PlanDecision, PlanStep,
    Timing, Touch,
};

pub const ROLE: &str = "audio-editor";
/// Monday 2 November 2026, 08:00 in Madrid (07:00 UTC): the first run is 09:00 that day.
pub const MON_0700: &str = "2026-11-02T07:00:00Z";

pub fn ms(text: &str) -> i64 {
    text.parse::<chrono::DateTime<chrono::Utc>>()
        .expect("an instant")
        .timestamp_millis()
}

pub fn set_clock(t: &Team, text: &str) {
    t.store.set_clock(Arc::new(ManualClock::at(ms(text))));
}

pub fn daily() -> Option<String> {
    Some("FREQ=DAILY;INTERVAL=1".to_string())
}

pub fn timing(rule: Option<String>, start: &str) -> Timing {
    Timing {
        cadence: cadence_of(rule.as_deref()).expect("cadence"),
        start_date: NaiveDate::parse_from_str(start, "%Y-%m-%d").expect("date"),
        time: NaiveTime::from_hms_opt(9, 0, 0).expect("time"),
        zone: Madrid,
    }
}

pub fn body(t: &Team, title: &str, text: &str) -> NewSchedule {
    NewSchedule {
        title: title.to_string(),
        conversation_id: t.conversation.clone(),
        role_id: ROLE.to_string(),
        content: Content::Body(text.to_string()),
        timing: timing(daily(), "2026-11-02"),
        steps: 1,
        plan_id: None,
    }
}

pub fn hire(t: &Team) {
    t.store
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
}

pub fn plan(t: &Team, steps: usize) -> String {
    let proposed = t
        .store
        .propose_plan(NewPlan {
            conversation_id: t.conversation.clone(),
            proposer_agent_id: t.lead.clone(),
            steps: (0..steps)
                .map(|n| PlanStep {
                    title: format!("Paso {n}"),
                    touches: vec![Touch::Files],
                })
                .collect(),
        })
        .expect("plan");
    t.store
        .decide_plan(&proposed.id, PlanDecision::Approve, None, "user")
        .expect("approve");
    proposed.id
}

pub fn launched(claim: Claim) -> excubia_core_chat::Launch {
    match claim {
        Claim::Launch(launch) => launch,
        other => panic!("expected a launch, got {other:?}"),
    }
}
