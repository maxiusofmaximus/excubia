//! The alert rules and the mode (F5). Each write and its `agenda_audit` line share one
//! transaction, so a refused rule leaves nothing behind. The mode is a `meta` row; a missing
//! row means `all`, the behaviour before rules existed.

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use super::agenda::{write_audit, Writer};
use crate::error::ChatError;
use crate::id;
use crate::model_alert::{
    decide, AlertAction, AlertDecision, AlertMode, AlertRule, AlertStatus, AlertSubject,
    AlertTrigger, NewAlertRule, RuleAuthor, ALERT_MODE_KEY, MAX_ALERT_RULES,
};
use crate::schema::{meta_value, set_meta};
use crate::seat::SeatActor;

const RULE_SELECT: &str = "SELECT id, trigger, agent, status_filter, action, enabled, created_by,
        created_at FROM alert_rule";

/// Who changes a rule or the mode: the app for the user, or a seat for Alfred.
#[derive(Clone, Copy, Debug)]
pub enum AlertWriter<'a> {
    Device(&'a str),
    Seat(&'a SeatActor),
}

impl<'a> AlertWriter<'a> {
    fn writer(self) -> Writer<'a> {
        match self {
            Self::Device(device_id) => Writer::device(device_id),
            Self::Seat(actor) => Writer::seat(actor),
        }
    }

    fn author(self) -> RuleAuthor {
        match self {
            Self::Device(_) => RuleAuthor::User,
            Self::Seat(_) => RuleAuthor::Alfred,
        }
    }
}

/// What the status read model shows of the rules: the mode and how many rules are on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlertSummary {
    pub mode: AlertMode,
    pub enabled_count: usize,
}

impl super::ChatStore {
    pub fn alert_mode(&self) -> Result<AlertMode, ChatError> {
        mode_of(&self.lock())
    }

    pub fn set_alert_mode(
        &self,
        by: AlertWriter<'_>,
        mode: AlertMode,
    ) -> Result<AlertMode, ChatError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        set_meta(&transaction, ALERT_MODE_KEY, mode.as_str())?;
        audit(&transaction, "alert_mode", "company", "set", by, &now)?;
        transaction.commit()?;
        Ok(mode)
    }

    pub fn alert_rules(&self) -> Result<Vec<AlertRule>, ChatError> {
        rules_in(&self.lock())
    }

    pub fn alert_summary(&self) -> Result<AlertSummary, ChatError> {
        let connection = self.lock();
        let enabled_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM alert_rule WHERE enabled = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(AlertSummary {
            mode: mode_of(&connection)?,
            enabled_count: usize::try_from(enabled_count).unwrap_or(0),
        })
    }

    pub fn create_alert_rule(
        &self,
        by: AlertWriter<'_>,
        new: NewAlertRule,
    ) -> Result<AlertRule, ChatError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        if count_rules(&transaction)? >= MAX_ALERT_RULES {
            return Err(ChatError::AlertRuleLimit);
        }
        if duplicate_exists(&transaction, &new)? {
            return Err(ChatError::AlertRuleExists);
        }
        let rule = AlertRule {
            id: id::new_id("ar"),
            trigger: new.trigger,
            agent: new.agent,
            status: new.status,
            action: new.action,
            enabled: true,
            author: by.author(),
            created_at: now.clone(),
        };
        insert_rule(&transaction, &rule)?;
        audit(&transaction, "alert_rule", &rule.id, "create", by, &now)?;
        transaction.commit()?;
        Ok(rule)
    }

    pub fn set_alert_rule_enabled(
        &self,
        by: AlertWriter<'_>,
        rule_id: &str,
        enabled: bool,
    ) -> Result<AlertRule, ChatError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        let mut rule = rule_in(&transaction, rule_id)?;
        transaction.execute(
            "UPDATE alert_rule SET enabled = ?2 WHERE id = ?1",
            params![rule_id, enabled],
        )?;
        let action = if enabled { "enable" } else { "disable" };
        audit(&transaction, "alert_rule", rule_id, action, by, &now)?;
        transaction.commit()?;
        rule.enabled = enabled;
        Ok(rule)
    }

    pub fn delete_alert_rule(&self, by: AlertWriter<'_>, rule_id: &str) -> Result<(), ChatError> {
        let now = self.now_text();
        let mut connection = self.lock();
        let transaction = connection.transaction()?;
        rule_in(&transaction, rule_id)?;
        transaction.execute("DELETE FROM alert_rule WHERE id = ?1", [rule_id])?;
        audit(&transaction, "alert_rule", rule_id, "delete", by, &now)?;
        transaction.commit()?;
        Ok(())
    }

    /// The verdict for a pane that changed to `status`. The agent is the name of the seat
    /// that owns the pane, as the chat shows it; a pane with no seat has no name.
    pub fn alert_for_pane(
        &self,
        pane_id: &str,
        status: AlertStatus,
    ) -> Result<AlertDecision, ChatError> {
        let connection = self.lock();
        let agent = agent_of_pane(&connection, pane_id)?;
        let subject = AlertSubject::Pane {
            agent: agent.as_deref(),
            status,
        };
        Ok(decide(
            mode_of(&connection)?,
            &rules_in(&connection)?,
            subject,
        ))
    }

    /// The verdict for a reminder that was just delivered.
    pub fn alert_for_reminder(&self) -> Result<AlertDecision, ChatError> {
        let connection = self.lock();
        let subject = AlertSubject::CalendarReminder;
        Ok(decide(
            mode_of(&connection)?,
            &rules_in(&connection)?,
            subject,
        ))
    }
}

fn audit(
    transaction: &Transaction<'_>,
    object_kind: &str,
    object_id: &str,
    action: &str,
    by: AlertWriter<'_>,
    at: &str,
) -> Result<(), ChatError> {
    let writer = by.writer();
    write_audit(
        transaction,
        object_kind,
        object_id,
        action,
        writer.kind,
        writer.id,
        at,
    )
}

fn mode_of(connection: &Connection) -> Result<AlertMode, ChatError> {
    match meta_value(connection, ALERT_MODE_KEY)? {
        None => Ok(AlertMode::All),
        Some(raw) => AlertMode::parse(&raw).ok_or(ChatError::Inconsistent("alerts_mode")),
    }
}

fn rules_in(connection: &Connection) -> Result<Vec<AlertRule>, ChatError> {
    let mut statement = connection.prepare(&format!("{RULE_SELECT} ORDER BY created_at, id"))?;
    let rules = statement
        .query_map([], rule_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rules)
}

fn rule_in(connection: &Connection, rule_id: &str) -> Result<AlertRule, ChatError> {
    connection
        .query_row(&format!("{RULE_SELECT} WHERE id = ?1"), [rule_id], rule_row)
        .optional()?
        .ok_or_else(|| ChatError::AlertRuleNotFound(rule_id.to_string()))
}

fn count_rules(transaction: &Transaction<'_>) -> Result<i64, ChatError> {
    Ok(transaction.query_row("SELECT COUNT(*) FROM alert_rule", [], |row| row.get(0))?)
}

/// An exact duplicate: the same trigger, agent, status and action, enabled or not.
fn duplicate_exists(transaction: &Transaction<'_>, new: &NewAlertRule) -> Result<bool, ChatError> {
    let found: Option<i64> = transaction
        .query_row(
            "SELECT 1 FROM alert_rule
              WHERE trigger = ?1 AND agent IS ?2 AND status_filter IS ?3 AND action = ?4",
            params![
                new.trigger.as_str(),
                new.agent,
                new.status.map(AlertStatus::as_str),
                new.action.as_str()
            ],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

fn insert_rule(transaction: &Transaction<'_>, rule: &AlertRule) -> Result<(), ChatError> {
    transaction.execute(
        "INSERT INTO alert_rule (id, trigger, agent, status_filter, action, enabled, created_by,
             created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7)",
        params![
            rule.id,
            rule.trigger.as_str(),
            rule.agent,
            rule.status.map(AlertStatus::as_str),
            rule.action.as_str(),
            rule.author.as_str(),
            rule.created_at
        ],
    )?;
    Ok(())
}

fn agent_of_pane(connection: &Connection, pane_id: &str) -> Result<Option<String>, ChatError> {
    Ok(connection
        .query_row(
            "SELECT name FROM agent_seat WHERE pane_id = ?1",
            [pane_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// A stored value the check constraints do not allow is a broken database, not a bad request.
fn rule_row(row: &Row<'_>) -> rusqlite::Result<AlertRule> {
    let trigger: String = row.get(1)?;
    let status: Option<String> = row.get(3)?;
    let action: String = row.get(4)?;
    let author: String = row.get(6)?;
    Ok(AlertRule {
        id: row.get(0)?,
        trigger: stored(1, &trigger, AlertTrigger::parse)?,
        agent: row.get(2)?,
        status: status
            .map(|raw| stored(3, &raw, AlertStatus::parse))
            .transpose()?,
        action: stored(4, &action, AlertAction::parse)?,
        enabled: row.get(5)?,
        author: stored(6, &author, RuleAuthor::parse)?,
        created_at: row.get(7)?,
    })
}

fn stored<T>(column: usize, raw: &str, parse: fn(&str) -> Option<T>) -> rusqlite::Result<T> {
    parse(raw).ok_or_else(|| {
        rusqlite::Error::InvalidColumnType(column, raw.to_string(), rusqlite::types::Type::Text)
    })
}

#[cfg(test)]
#[path = "alerts_tests.rs"]
mod tests;
