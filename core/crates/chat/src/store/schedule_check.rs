//! The checks a scheduled work passes on the way in (F4-CORE): the clock, the steps, the text
//! or recipe, the plan and the first run. Each runs inside the write's transaction, so a plan
//! cannot be taken by two saves at once.

use rusqlite::{params, Connection, OptionalExtension};

use super::lookup::conversation_exists;
use super::plans::load;
use super::schedules::instant_of;
use crate::clock;
use crate::error::ChatError;
use crate::model_plan::PlanStatus;
use crate::model_recipe::{RecipeText, RECIPE_TITLE_CHARS};
use crate::model_schedule::{check_steps, Cadence, Content, NewSchedule, Timing};
use crate::policy::PlanRules;
use crate::recipe_guard::admit;
use crate::recurrence::Freq;
use crate::schedule_due::next_after;

/// A schedule as checked, ready to store.
pub(super) struct Checked {
    pub(super) conversation_id: String,
    pub(super) role_id: String,
    pub(super) title: String,
    pub(super) body: Option<String>,
    pub(super) recipe_id: Option<String>,
    pub(super) timing: Timing,
    pub(super) steps: u32,
    pub(super) plan_id: Option<String>,
}

/// Checks what a create or an edit carries, in a fixed order, before any write.
pub(super) fn check_new(
    connection: &Connection,
    rules: &PlanRules,
    new: &NewSchedule,
    schedule_id: Option<&str>,
) -> Result<Checked, ChatError> {
    let steps = check_steps(new.steps)?;
    if let Cadence::Repeat(rule) = &new.timing.cadence {
        if rule.freq == Freq::Yearly {
            return Err(ChatError::InvalidSchedule("rule"));
        }
    }
    if !conversation_exists(connection, &new.conversation_id)? {
        return Err(ChatError::NoSuchConversation);
    }
    let project: Option<String> = connection.query_row(
        "SELECT project_id FROM conversation WHERE id = ?1",
        [&new.conversation_id],
        |row| row.get(0),
    )?;
    if project.is_none() {
        return Err(ChatError::ConversationWithoutProject);
    }
    let (title, body, recipe_id) = check_content(connection, new)?;
    let plan_id = check_plan(
        connection,
        rules,
        schedule_id,
        &new.conversation_id,
        steps,
        new.plan_id.as_deref(),
    )?;
    Ok(Checked {
        conversation_id: new.conversation_id.clone(),
        role_id: new.role_id.clone(),
        title,
        body,
        recipe_id,
        timing: new.timing.clone(),
        steps,
        plan_id,
    })
}

/// The title, body and recipe of a schedule. A body is checked like a recipe's text, secrets
/// included; a recipe must exist and belong to the oficio the schedule runs.
fn check_content(
    connection: &Connection,
    new: &NewSchedule,
) -> Result<(String, Option<String>, Option<String>), ChatError> {
    match &new.content {
        Content::Body(body) => {
            let text = crate::model_recipe::check_text(&RecipeText {
                title: new.title.clone(),
                body: body.clone(),
            })?;
            let text = admit(&text, false)?;
            Ok((text.title, Some(text.body), None))
        }
        Content::Recipe(recipe_id) => {
            let title = check_title(&new.title)?;
            let role: Option<String> = connection
                .query_row(
                    "SELECT role_id FROM recipe WHERE id = ?1",
                    [recipe_id],
                    |row| row.get(0),
                )
                .optional()?;
            match role {
                Some(role) if role == new.role_id => Ok((title, None, Some(recipe_id.clone()))),
                _ => Err(ChatError::InvalidSchedule("recipe")),
            }
        }
    }
}

fn check_title(title: &str) -> Result<String, ChatError> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > RECIPE_TITLE_CHARS {
        return Err(ChatError::InvalidSchedule("title"));
    }
    Ok(title.to_string())
}

/// The plan a schedule runs under, or `None` when it needs none. The plan must be approved, in
/// the same conversation, and held by no other live schedule.
fn check_plan(
    connection: &Connection,
    rules: &PlanRules,
    schedule_id: Option<&str>,
    conversation_id: &str,
    steps: u32,
    plan_id: Option<&str>,
) -> Result<Option<String>, ChatError> {
    let Some(plan_id) = plan_id else {
        return if rules.requires_plan(steps) {
            Err(ChatError::PlanRequired)
        } else {
            Ok(None)
        };
    };
    let plan = load(connection, plan_id)?;
    if plan.status != PlanStatus::Approved || plan.conversation_id != conversation_id {
        return Err(ChatError::PlanNotApproved);
    }
    let held: i64 = connection.query_row(
        "SELECT COUNT(*) FROM schedule
          WHERE plan_id = ?1 AND status != 'done' AND id IS NOT ?2",
        params![plan_id, schedule_id],
        |row| row.get(0),
    )?;
    if held > 0 {
        return Err(ChatError::PlanLinked);
    }
    Ok(Some(plan_id.to_string()))
}

/// The first run after `now_ms`, as the text the store keeps, or `ScheduleNoFutureRun`.
pub(super) fn first_run(timing: &Timing, now_ms: i64) -> Result<String, ChatError> {
    let now = instant_of(now_ms)?;
    next_after(timing, now)
        .map(|at| clock::iso(at.timestamp_millis()))
        .ok_or(ChatError::ScheduleNoFutureRun)
}
