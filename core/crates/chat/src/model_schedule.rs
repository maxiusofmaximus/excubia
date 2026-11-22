//! Scheduled work (F4-CORE): a text, or a recipe, that a person asks an oficio to run on a
//! clock. This file holds what a schedule is and the limits it keeps. When one is due is in
//! [`crate::schedule_due`]; what it may do at that moment is decided by the store.

use chrono::{NaiveDate, NaiveTime};
use chrono_tz::Tz;
use serde::Serialize;

use crate::error::ChatError;
use crate::model_plan::MAX_STEPS as MAX_PLAN_STEPS;
use crate::recurrence::{Freq, Rule};

/// The schedules that may be active at once.
pub const MAX_ACTIVE_SCHEDULES: i64 = 20;
/// The runs one schedule keeps; the older ones are dropped.
pub const HISTORY_KEEP: i64 = 50;
/// A run this late or later is missed: it is not launched, the person is told.
pub const LATE_LIMIT_SECS: i64 = 120;

/// The brakes on launches (`[schedules]` of `approvals.toml`). A run that a brake stops is
/// recorded as skipped, never queued.
pub const DEFAULT_PER_HOUR_PER_AGENT: i64 = 1;
pub const DEFAULT_PER_DAY_PER_AGENT: i64 = 4;
pub const DEFAULT_PER_DAY_TOTAL: i64 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleStatus {
    Active,
    Paused,
    Done,
}

impl ScheduleStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Done => "done",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "done" => Some(Self::Done),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Started,
    SkippedCap,
    SkippedOverlap,
    MissedOffline,
    Failed,
}

impl RunOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::SkippedCap => "skipped_cap",
            Self::SkippedOverlap => "skipped_overlap",
            Self::MissedOffline => "missed_offline",
            Self::Failed => "failed",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "started" => Some(Self::Started),
            "skipped_cap" => Some(Self::SkippedCap),
            "skipped_overlap" => Some(Self::SkippedOverlap),
            "missed_offline" => Some(Self::MissedOffline),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// What decides a due instant: a run the host saw on time, or one it saw too late.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Launch,
    Missed,
}

/// When a schedule runs: once on `start_date`, or by a rule from `start_date` on. The clock
/// is `time` in `zone`, the company's zone as the schedule was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timing {
    pub cadence: Cadence,
    pub start_date: NaiveDate,
    pub time: NaiveTime,
    pub zone: Tz,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cadence {
    Once,
    Repeat(Rule),
}

/// What a schedule carries: its own text, or the id of a recipe of its oficio.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Body(String),
    Recipe(String),
}

/// One schedule as the store keeps it and the API shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Schedule {
    pub id: String,
    pub title: String,
    pub conversation_id: String,
    pub role_id: String,
    pub body: Option<String>,
    pub recipe_id: Option<String>,
    pub rule: Option<String>,
    pub start_date: String,
    pub time_local: String,
    pub timezone: String,
    pub steps: u32,
    pub status: ScheduleStatus,
    pub plan_id: Option<String>,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One due instant of a schedule and what became of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScheduleRun {
    pub id: String,
    pub schedule_id: String,
    pub due_at: String,
    pub started_at: Option<String>,
    pub outcome: RunOutcome,
    pub delegation_id: Option<String>,
}

/// What a create or an edit carries, checked by the store before any write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSchedule {
    pub title: String,
    pub conversation_id: String,
    pub role_id: String,
    pub content: Content,
    pub timing: Timing,
    pub steps: u32,
    pub plan_id: Option<String>,
}

/// A clock time as the person writes it: `HH:MM`, nothing else.
pub fn parse_time_local(text: &str) -> Option<NaiveTime> {
    if text.len() != 5 {
        return None;
    }
    NaiveTime::parse_from_str(text, "%H:%M").ok()
}

/// A start date as the person writes it: `YYYY-MM-DD`, nothing else.
pub fn parse_start_date(text: &str) -> Option<NaiveDate> {
    if text.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()
}

/// The cadence a rule text names. No rule is a one-time run; a rule must repeat daily,
/// weekly or monthly, the four kinds the schedules offer with one-time.
pub fn cadence_of(rule: Option<&str>) -> Result<Cadence, ChatError> {
    let Some(text) = rule.map(str::trim).filter(|text| !text.is_empty()) else {
        return Ok(Cadence::Once);
    };
    let rule = Rule::parse(text).map_err(|_| ChatError::InvalidSchedule("rule"))?;
    match rule.freq {
        Freq::Yearly => Err(ChatError::InvalidSchedule("rule")),
        _ => Ok(Cadence::Repeat(rule)),
    }
}

/// The rule text a cadence keeps: none for a one-time run.
pub fn rule_text(cadence: &Cadence) -> Option<String> {
    match cadence {
        Cadence::Once => None,
        Cadence::Repeat(rule) => Some(rule.to_string()),
    }
}

/// The timing a stored schedule names, read back from its columns.
pub fn timing_of(
    start_date: &str,
    time_local: &str,
    timezone: &str,
    rule: Option<&str>,
) -> Result<Timing, ChatError> {
    let invalid = || ChatError::InvalidSchedule("time");
    Ok(Timing {
        cadence: cadence_of(rule)?,
        start_date: parse_start_date(start_date).ok_or_else(invalid)?,
        time: parse_time_local(time_local).ok_or_else(invalid)?,
        zone: crate::timezone::parse_zone(timezone).ok_or_else(invalid)?,
    })
}

/// Steps are a count from one up to the plan's own limit.
pub fn check_steps(steps: u32) -> Result<u32, ChatError> {
    if steps == 0 || steps as usize > MAX_PLAN_STEPS {
        return Err(ChatError::InvalidSchedule("steps"));
    }
    Ok(steps)
}
