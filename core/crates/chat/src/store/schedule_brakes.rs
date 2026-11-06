//! The brakes on scheduled launches (F4-CORE): a run that an overlap or a cap stops is recorded,
//! never queued. Both are counted from the runs' own rows, so a restart keeps them.

use rusqlite::{params, Connection, OptionalExtension};

use crate::clock;
use crate::error::ChatError;
use crate::model_schedule::Schedule;
use crate::policy::ScheduleRules;

/// A run started, but its delegation not yet known for this long, is still in flight. A host
/// that died mid-launch leaves such a run behind; after this it no longer blocks the next one.
const IN_FLIGHT_SECS: i64 = 5 * 60;

/// Whether the last started run of a schedule is still going: its delegation is active, or it
/// has no delegation yet and started less than `IN_FLIGHT_SECS` ago.
pub(super) fn overlaps(
    connection: &Connection,
    schedule_id: &str,
    now_ms: i64,
) -> Result<bool, ChatError> {
    let last: Option<(Option<String>, Option<String>)> = connection
        .query_row(
            "SELECT delegation_id, started_at FROM schedule_run
              WHERE schedule_id = ?1 AND outcome = 'started'
              ORDER BY rowid DESC LIMIT 1",
            [schedule_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((delegation_id, started_at)) = last else {
        return Ok(false);
    };
    match delegation_id {
        Some(delegation_id) => {
            let state: Option<String> = connection
                .query_row(
                    "SELECT state FROM delegation WHERE id = ?1",
                    [&delegation_id],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(matches!(
                state.as_deref(),
                Some("queued" | "running" | "waiting")
            ))
        }
        None => Ok(started_at
            .as_deref()
            .and_then(clock::parse_iso)
            .is_some_and(|at| now_ms - at < IN_FLIGHT_SECS * 1000)),
    }
}

/// Whether a launch would pass the brakes: the oficio's runs in the last hour and day, and the
/// company's runs in the last day, all counted as started runs.
pub(super) fn over_cap(
    connection: &Connection,
    schedule: &Schedule,
    now_ms: i64,
    rules: &ScheduleRules,
) -> Result<bool, ChatError> {
    let hour = started_since(connection, Some(&schedule.role_id), now_ms - 3_600_000)?;
    let day_agent = started_since(connection, Some(&schedule.role_id), now_ms - 86_400_000)?;
    let day_total = started_since(connection, None, now_ms - 86_400_000)?;
    Ok(hour >= rules.per_hour_per_agent
        || day_agent >= rules.per_day_per_agent
        || day_total >= rules.per_day_total)
}

fn started_since(
    connection: &Connection,
    role: Option<&str>,
    since_ms: i64,
) -> Result<i64, ChatError> {
    let since = clock::iso(since_ms);
    let count = match role {
        Some(role) => connection.query_row(
            "SELECT COUNT(*) FROM schedule_run r JOIN schedule s ON s.id = r.schedule_id
              WHERE s.role_id = ?1 AND r.outcome = 'started' AND r.started_at > ?2",
            params![role, since],
            |row| row.get(0),
        )?,
        None => connection.query_row(
            "SELECT COUNT(*) FROM schedule_run WHERE outcome = 'started' AND started_at > ?1",
            [since],
            |row| row.get(0),
        )?,
    };
    Ok(count)
}
