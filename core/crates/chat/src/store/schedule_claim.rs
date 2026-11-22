//! The runs of scheduled work (F4-CORE): what the clock decides at a due instant, and what a
//! person's «run now» decides. Each decision is written in the transaction that moves the
//! schedule to its next run, so one due instant is never decided twice. The launch itself is
//! the host's: the store hands it a [`Launch`] and takes back the delegation it got.

use rusqlite::{params, Connection, OptionalExtension, Transaction};

use super::recipe_run::run_of;
use super::schedule_brakes::{over_cap, overlaps};
use super::schedules::{instant_of, schedule_by_id};
use crate::clock;
use crate::error::ChatError;
use crate::id;
use crate::model_recipe::RecipeRun;
use crate::model_schedule::{
    timing_of, RunOutcome, Schedule, ScheduleRun, ScheduleStatus, Timing, Verdict, HISTORY_KEEP,
};
use crate::policy::ScheduleRules;
use crate::schedule_due::{next_after, verdict};

/// A run the host must start: the delegation's body, hire and conversation, and the run
/// row it is recorded under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub run_id: String,
    pub schedule_id: String,
    pub steps: u32,
    pub run: RecipeRun,
}

/// What a manual run came to: a launch the host must start, or a run recorded without one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Claim {
    Launch(Launch),
    Recorded(ScheduleRun),
}

/// A message the store posts once its transaction is out: a missed run, or a capped one.
struct Notice {
    conversation_id: String,
    text: String,
}

enum Decision {
    Launch,
    Skip(RunOutcome),
}

impl super::ChatStore {
    /// The brakes of the policy file as they stand now.
    pub(super) fn schedule_rules(&self) -> ScheduleRules {
        let mut policy = self
            .policy
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        policy.current().schedules().clone()
    }

    /// Decides every schedule whose run has come due. A run the host reached on time is
    /// launched; a later one is missed, and the schedule moves past the missed instants to its
    /// next run after now. Returns the runs to start, and posts the notices.
    pub fn claim_due_schedules(&self) -> Result<Vec<Launch>, ChatError> {
        let rules = self.schedule_rules();
        let now_ms = self.now_ms();
        let now = instant_of(now_ms)?;
        let mut launches = Vec::new();
        let mut notices = Vec::new();
        {
            let mut connection = self.lock();
            let transaction = connection.transaction()?;
            for schedule_id in due_ids(&transaction, now_ms)? {
                let schedule = schedule_by_id(&transaction, &schedule_id)?;
                let Some(due_text) = schedule.next_run_at.clone() else {
                    continue;
                };
                let due_ms =
                    clock::parse_iso(&due_text).ok_or(ChatError::InvalidSchedule("time"))?;
                let due = instant_of(due_ms)?;
                let timing = timing_of(
                    &schedule.start_date,
                    &schedule.time_local,
                    &schedule.timezone,
                    schedule.rule.as_deref(),
                )?;
                match verdict(due, now) {
                    Verdict::Missed => {
                        record(
                            &transaction,
                            &schedule,
                            &due_text,
                            None,
                            RunOutcome::MissedOffline,
                            None,
                        )?;
                        notices.push(notice(&schedule, missed_text(&schedule)));
                    }
                    Verdict::Launch => {
                        match self.decide(&transaction, &schedule, now_ms, &rules)? {
                            Decision::Launch => {
                                match start(&transaction, &schedule, &due_text, now_ms) {
                                    Ok(launch) => launches.push(launch),
                                    Err(_) => {
                                        record(
                                            &transaction,
                                            &schedule,
                                            &due_text,
                                            Some(&clock::iso(now_ms)),
                                            RunOutcome::Failed,
                                            None,
                                        )?;
                                    }
                                }
                            }
                            Decision::Skip(outcome) => {
                                record(&transaction, &schedule, &due_text, None, outcome, None)?;
                                if outcome == RunOutcome::SkippedCap {
                                    notices.push(notice(&schedule, capped_text(&schedule)));
                                }
                            }
                        }
                    }
                }
                advance(&transaction, &schedule.id, &timing, now_ms)?;
            }
            transaction.commit()?;
        }
        self.post_notices(notices);
        Ok(launches)
    }

    /// «Run now»: the same brakes and overlap as a due run, but the schedule keeps its rhythm.
    /// A one-time schedule is done once it has run. A text that cannot start is an error.
    pub fn claim_schedule_now(&self, schedule_id: &str) -> Result<Claim, ChatError> {
        let rules = self.schedule_rules();
        let now_ms = self.now_ms();
        let now = clock::iso(now_ms);
        let mut notices = Vec::new();
        let claim = {
            let mut connection = self.lock();
            let transaction = connection.transaction()?;
            let schedule = schedule_by_id(&transaction, schedule_id)?;
            if schedule.status == ScheduleStatus::Done {
                return Err(ChatError::ScheduleDone);
            }
            let claim = match self.decide(&transaction, &schedule, now_ms, &rules)? {
                Decision::Launch => Claim::Launch(start(&transaction, &schedule, &now, now_ms)?),
                Decision::Skip(outcome) => {
                    let run = record(&transaction, &schedule, &now, None, outcome, None)?;
                    if outcome == RunOutcome::SkippedCap {
                        notices.push(notice(&schedule, capped_text(&schedule)));
                    }
                    Claim::Recorded(run)
                }
            };
            if matches!(claim, Claim::Launch(_)) {
                finish_once(&transaction, &schedule)?;
            }
            transaction.commit()?;
            claim
        };
        self.post_notices(notices);
        Ok(claim)
    }

    /// Settles a launch: the delegation the host started, or `None` when it could not start.
    pub fn settle_schedule_run(
        &self,
        run_id: &str,
        delegation_id: Option<&str>,
    ) -> Result<(), ChatError> {
        let connection = self.lock();
        let changed = match delegation_id {
            Some(delegation_id) => connection.execute(
                "UPDATE schedule_run SET delegation_id = ?2 WHERE id = ?1",
                params![run_id, delegation_id],
            )?,
            None => connection.execute(
                "UPDATE schedule_run SET outcome = 'failed' WHERE id = ?1",
                [run_id],
            )?,
        };
        if changed == 0 {
            return Err(ChatError::NoSuchRun(run_id.to_string()));
        }
        Ok(())
    }

    /// The decision for a run that is due or asked for now: overlap first, then the brakes.
    fn decide(
        &self,
        connection: &Connection,
        schedule: &Schedule,
        now_ms: i64,
        rules: &ScheduleRules,
    ) -> Result<Decision, ChatError> {
        if overlaps(connection, &schedule.id, now_ms)? {
            return Ok(Decision::Skip(RunOutcome::SkippedOverlap));
        }
        if over_cap(connection, schedule, now_ms, rules)? {
            return Ok(Decision::Skip(RunOutcome::SkippedCap));
        }
        Ok(Decision::Launch)
    }

    fn post_notices(&self, notices: Vec<Notice>) {
        for notice in notices {
            if let Err(error) = self.append_alfred_message(&notice.conversation_id, &notice.text) {
                eprintln!("host: could not post a schedule notice: {error}");
            }
        }
    }
}

/// The schedules whose next run is due at `now_ms`, earliest first.
fn due_ids(connection: &Connection, now_ms: i64) -> Result<Vec<String>, ChatError> {
    let mut statement = connection.prepare(
        "SELECT id FROM schedule
          WHERE status = 'active' AND next_run_at IS NOT NULL AND next_run_at <= ?1
          ORDER BY next_run_at, id",
    )?;
    let rows = statement.query_map([clock::iso(now_ms)], |row| row.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Writes a run row and keeps the schedule's history at `HISTORY_KEEP` rows.
fn record(
    transaction: &Transaction<'_>,
    schedule: &Schedule,
    due_at: &str,
    started_at: Option<&str>,
    outcome: RunOutcome,
    delegation_id: Option<&str>,
) -> Result<ScheduleRun, ChatError> {
    let run_id = id::new_id("srn");
    transaction.execute(
        "INSERT INTO schedule_run (id, schedule_id, due_at, started_at, outcome, delegation_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            run_id,
            schedule.id,
            due_at,
            started_at,
            outcome.as_str(),
            delegation_id
        ],
    )?;
    transaction.execute(
        "DELETE FROM schedule_run WHERE schedule_id = ?1 AND rowid NOT IN (
             SELECT rowid FROM schedule_run WHERE schedule_id = ?1
              ORDER BY rowid DESC LIMIT ?2)",
        params![schedule.id, HISTORY_KEEP],
    )?;
    Ok(ScheduleRun {
        id: run_id,
        schedule_id: schedule.id.clone(),
        due_at: due_at.to_string(),
        started_at: started_at.map(str::to_string),
        outcome,
        delegation_id: delegation_id.map(str::to_string),
    })
}

/// Writes a started run and builds what the host launches. The text of a recipe is read now,
/// so an edit made before the run is the one that runs.
fn start(
    transaction: &Transaction<'_>,
    schedule: &Schedule,
    due_at: &str,
    now_ms: i64,
) -> Result<Launch, ChatError> {
    let now = clock::iso(now_ms);
    let task = match (&schedule.body, &schedule.recipe_id) {
        (Some(body), _) => body.clone(),
        (None, Some(recipe_id)) => transaction
            .query_row(
                "SELECT body FROM recipe WHERE id = ?1",
                [recipe_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| ChatError::NoSuchRecipe(recipe_id.clone()))?,
        (None, None) => return Err(ChatError::InvalidSchedule("content")),
    };
    let alfred_id = super::lookup::alfred_id(transaction)?;
    let run = run_of(
        transaction,
        alfred_id,
        &schedule.conversation_id,
        &schedule.role_id,
        task,
        schedule.recipe_id.clone(),
    )?;
    let row = record(
        transaction,
        schedule,
        due_at,
        Some(&now),
        RunOutcome::Started,
        None,
    )?;
    transaction.execute(
        "UPDATE schedule SET last_run_at = ?2 WHERE id = ?1",
        params![schedule.id, now],
    )?;
    Ok(Launch {
        run_id: row.id,
        schedule_id: schedule.id.clone(),
        steps: schedule.steps,
        run,
    })
}

/// Moves a schedule past `now`: its next run, or done when a one-time run has no next one.
fn advance(
    transaction: &Transaction<'_>,
    schedule_id: &str,
    timing: &Timing,
    now_ms: i64,
) -> Result<(), ChatError> {
    let next = next_after(timing, instant_of(now_ms)?);
    match next {
        Some(at) => transaction.execute(
            "UPDATE schedule SET next_run_at = ?2 WHERE id = ?1",
            params![schedule_id, clock::iso(at.timestamp_millis())],
        )?,
        None => transaction.execute(
            "UPDATE schedule SET status = 'done', next_run_at = NULL WHERE id = ?1",
            [schedule_id],
        )?,
    };
    Ok(())
}

/// A one-time schedule is done once its one run has been started by hand.
fn finish_once(transaction: &Transaction<'_>, schedule: &Schedule) -> Result<(), ChatError> {
    if schedule.rule.is_none() {
        transaction.execute(
            "UPDATE schedule SET status = 'done', next_run_at = NULL WHERE id = ?1",
            [&schedule.id],
        )?;
    }
    Ok(())
}

fn notice(schedule: &Schedule, text: String) -> Notice {
    Notice {
        conversation_id: schedule.conversation_id.clone(),
        text,
    }
}

/// The notice of a missed run: the title and the hour, never the text of the run.
fn missed_text(schedule: &Schedule) -> String {
    format!(
        "Se saltó “{}” porque el PC estaba apagado a las {}. Puedes lanzarla ahora.",
        schedule.title, schedule.time_local
    )
}

fn capped_text(schedule: &Schedule) -> String {
    format!(
        "No se lanzó “{}”: se alcanzó el tope de ejecuciones programadas. Puedes lanzarla ahora.",
        schedule.title
    )
}
