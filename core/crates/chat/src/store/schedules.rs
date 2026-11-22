//! Scheduled work (F4-CORE): what a person schedules and how it is checked on the way in.
//! Every check runs inside the write's transaction, so a plan cannot be taken by two saves at
//! once. A plan is checked when its schedule is saved only: a plan's decision is final, so the
//! runs do not check it again. The runs themselves are in [`super::schedule_claim`].

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use super::lookup::company_id;
use super::schedule_check::{check_new, first_run, Checked};
use crate::clock;
use crate::error::ChatError;
use crate::id;
use crate::model_schedule::{
    rule_text, NewSchedule, RunOutcome, Schedule, ScheduleRun, ScheduleStatus, Timing,
    HISTORY_KEEP, MAX_ACTIVE_SCHEDULES,
};

const SCHEDULE_COLUMNS: &str = "id, title, conversation_id, role_id, body, recipe_id, rule,
       start_date, time_local, timezone, steps, status, plan_id, next_run_at, last_run_at,
       created_at, updated_at";

impl super::ChatStore {
    pub fn schedules(&self, role: Option<&str>) -> Result<Vec<Schedule>, ChatError> {
        let connection = self.lock();
        let clause = match role {
            Some(_) => "WHERE role_id = ?1 ORDER BY created_at, id",
            None => "ORDER BY created_at, id",
        };
        let mut statement =
            connection.prepare(&format!("SELECT {SCHEDULE_COLUMNS} FROM schedule {clause}"))?;
        let rows = statement.query_map(rusqlite::params_from_iter(role), schedule_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn schedule(&self, schedule_id: &str) -> Result<Schedule, ChatError> {
        schedule_by_id(&self.lock(), schedule_id)
    }

    /// The runs of one schedule, newest first, at most `HISTORY_KEEP`.
    pub fn schedule_runs(&self, schedule_id: &str) -> Result<Vec<ScheduleRun>, ChatError> {
        let connection = self.lock();
        schedule_by_id(&connection, schedule_id)?;
        let mut statement = connection.prepare(
            "SELECT id, schedule_id, due_at, started_at, outcome, delegation_id
               FROM schedule_run WHERE schedule_id = ?1
              ORDER BY rowid DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![schedule_id, HISTORY_KEEP], run_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Keep a new schedule, active from its first run after now. A one-time run that has
    /// passed is refused, and so is a plan that does not cover its steps.
    pub fn create_schedule(&self, new: NewSchedule) -> Result<Schedule, ChatError> {
        let rules = self.plan_rules();
        let now_ms = self.now_ms();
        let now = clock::iso(now_ms);
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let checked = check_new(&transaction, &rules, &new, None)?;
        if active_count(&transaction)? >= MAX_ACTIVE_SCHEDULES {
            return Err(ChatError::ScheduleLimit);
        }
        let next = first_run(&checked.timing, now_ms)?;
        let schedule_id = id::new_id("sch");
        insert(&transaction, &schedule_id, &checked, &now, &next)?;
        let schedule = schedule_by_id(&transaction, &schedule_id)?;
        transaction.commit()?;
        Ok(schedule)
    }

    /// Replace what a schedule says. Its runs keep their history. A changed text drops the
    /// plan it was approved with: the same plan id is refused, and a new one must be approved.
    pub fn update_schedule(
        &self,
        schedule_id: &str,
        new: NewSchedule,
    ) -> Result<Schedule, ChatError> {
        let rules = self.plan_rules();
        let now_ms = self.now_ms();
        let now = clock::iso(now_ms);
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let current = schedule_by_id(&transaction, schedule_id)?;
        if current.status == ScheduleStatus::Done {
            return Err(ChatError::ScheduleDone);
        }
        let checked = check_new(&transaction, &rules, &new, Some(schedule_id))?;
        let text_changed = current.body != checked.body || current.recipe_id != checked.recipe_id;
        if text_changed && checked.plan_id.is_some() && checked.plan_id == current.plan_id {
            return Err(ChatError::PlanExpired);
        }
        let next = first_run(&checked.timing, now_ms)?;
        transaction.execute(
            "UPDATE schedule SET title = ?2, conversation_id = ?3, role_id = ?4, body = ?5,
                    recipe_id = ?6, rule = ?7, start_date = ?8, time_local = ?9, timezone = ?10,
                    steps = ?11, plan_id = ?12, next_run_at = ?13, updated_at = ?14
              WHERE id = ?1",
            params![
                schedule_id,
                checked.title,
                checked.conversation_id,
                checked.role_id,
                checked.body,
                checked.recipe_id,
                rule_text(&checked.timing.cadence),
                start_text(&checked.timing),
                time_text(&checked.timing),
                checked.timing.zone.name(),
                checked.steps,
                checked.plan_id,
                next,
                now,
            ],
        )?;
        let schedule = schedule_by_id(&transaction, schedule_id)?;
        transaction.commit()?;
        Ok(schedule)
    }

    /// Stop the runs of a schedule until it is resumed. Its history is kept.
    pub fn pause_schedule(&self, schedule_id: &str) -> Result<Schedule, ChatError> {
        let now = clock::iso(self.now_ms());
        let connection = self.lock();
        let current = schedule_by_id(&connection, schedule_id)?;
        match current.status {
            ScheduleStatus::Done => Err(ChatError::ScheduleDone),
            ScheduleStatus::Paused => Ok(current),
            ScheduleStatus::Active => {
                connection.execute(
                    "UPDATE schedule SET status = 'paused', updated_at = ?2 WHERE id = ?1",
                    params![schedule_id, now],
                )?;
                schedule_by_id(&connection, schedule_id)
            }
        }
    }

    /// Start the runs again from the first run after now: the missed ones are not made up.
    pub fn resume_schedule(&self, schedule_id: &str) -> Result<Schedule, ChatError> {
        let now_ms = self.now_ms();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let current = schedule_by_id(&transaction, schedule_id)?;
        match current.status {
            ScheduleStatus::Done => return Err(ChatError::ScheduleDone),
            ScheduleStatus::Active => return Ok(current),
            ScheduleStatus::Paused => {}
        }
        if active_count(&transaction)? >= MAX_ACTIVE_SCHEDULES {
            return Err(ChatError::ScheduleLimit);
        }
        let timing = crate::model_schedule::timing_of(
            &current.start_date,
            &current.time_local,
            &current.timezone,
            current.rule.as_deref(),
        )?;
        let next = first_run(&timing, now_ms)?;
        transaction.execute(
            "UPDATE schedule SET status = 'active', next_run_at = ?2, updated_at = ?3
              WHERE id = ?1",
            params![schedule_id, next, clock::iso(now_ms)],
        )?;
        let schedule = schedule_by_id(&transaction, schedule_id)?;
        transaction.commit()?;
        Ok(schedule)
    }

    /// Remove a schedule and its history.
    pub fn delete_schedule(&self, schedule_id: &str) -> Result<(), ChatError> {
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        schedule_by_id(&transaction, schedule_id)?;
        transaction.execute(
            "DELETE FROM schedule_run WHERE schedule_id = ?1",
            [schedule_id],
        )?;
        transaction.execute("DELETE FROM schedule WHERE id = ?1", [schedule_id])?;
        transaction.commit()?;
        Ok(())
    }
}

/// The instant of a clock reading in milliseconds; the store's clocks never leave this range.
pub(super) fn instant_of(now_ms: i64) -> Result<DateTime<Utc>, ChatError> {
    DateTime::<Utc>::from_timestamp_millis(now_ms).ok_or(ChatError::InvalidSchedule("time"))
}

fn start_text(timing: &Timing) -> String {
    timing.start_date.format("%Y-%m-%d").to_string()
}

fn time_text(timing: &Timing) -> String {
    timing.time.format("%H:%M").to_string()
}

fn active_count(connection: &Connection) -> Result<i64, ChatError> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM schedule WHERE status = 'active'",
        [],
        |row| row.get(0),
    )?)
}

fn insert(
    transaction: &Transaction<'_>,
    schedule_id: &str,
    checked: &Checked,
    now: &str,
    next_run_at: &str,
) -> Result<(), ChatError> {
    transaction.execute(
        "INSERT INTO schedule
            (id, company_id, conversation_id, role_id, title, body, recipe_id, rule,
             start_date, time_local, timezone, steps, status, plan_id, next_run_at,
             created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'active', ?13, ?14, ?15, ?15)",
        params![
            schedule_id,
            company_id(transaction)?,
            checked.conversation_id,
            checked.role_id,
            checked.title,
            checked.body,
            checked.recipe_id,
            rule_text(&checked.timing.cadence),
            start_text(&checked.timing),
            time_text(&checked.timing),
            checked.timing.zone.name(),
            checked.steps,
            checked.plan_id,
            next_run_at,
            now,
        ],
    )?;
    Ok(())
}

pub(super) fn schedule_by_id(
    connection: &Connection,
    schedule_id: &str,
) -> Result<Schedule, ChatError> {
    connection
        .query_row(
            &format!("SELECT {SCHEDULE_COLUMNS} FROM schedule WHERE id = ?1"),
            [schedule_id],
            schedule_row,
        )
        .optional()?
        .ok_or_else(|| ChatError::NoSuchSchedule(schedule_id.to_string()))
}

fn schedule_row(row: &Row<'_>) -> rusqlite::Result<Schedule> {
    let status: String = row.get(11)?;
    let steps: i64 = row.get(10)?;
    Ok(Schedule {
        id: row.get(0)?,
        title: row.get(1)?,
        conversation_id: row.get(2)?,
        role_id: row.get(3)?,
        body: row.get(4)?,
        recipe_id: row.get(5)?,
        rule: row.get(6)?,
        start_date: row.get(7)?,
        time_local: row.get(8)?,
        timezone: row.get(9)?,
        steps: steps as u32,
        status: ScheduleStatus::from_db(&status).unwrap_or(ScheduleStatus::Paused),
        plan_id: row.get(12)?,
        next_run_at: row.get(13)?,
        last_run_at: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}

pub(super) fn run_row(row: &Row<'_>) -> rusqlite::Result<ScheduleRun> {
    let outcome: String = row.get(4)?;
    Ok(ScheduleRun {
        id: row.get(0)?,
        schedule_id: row.get(1)?,
        due_at: row.get(2)?,
        started_at: row.get(3)?,
        outcome: RunOutcome::from_db(&outcome).unwrap_or(RunOutcome::Failed),
        delegation_id: row.get(5)?,
    })
}
