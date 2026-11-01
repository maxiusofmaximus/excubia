//! The user's alert rules (F5): which agent changes and which reminders may reach the phone
//! as a live `alert` frame. The verdict is pure: the store reads the mode and the rules, and
//! the host acts on [`decide`]. A `mute` that matches always wins over a `notify`.

use serde_json::{json, Value};

use crate::text::agent_name;

/// The most rules a company keeps; the next one is refused with `rule_limit`.
pub const MAX_ALERT_RULES: i64 = 100;

/// The `meta` key that holds the mode; a missing key means [`AlertMode::All`].
pub const ALERT_MODE_KEY: &str = "alerts_mode";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertMode {
    /// Blocked and done changes of any agent alert, as they always did.
    All,
    /// Only the changes that a `notify` rule names alert.
    RulesOnly,
}

impl AlertMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::RulesOnly => "rules_only",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "all" => Some(Self::All),
            "rules_only" => Some(Self::RulesOnly),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertTrigger {
    PaneStatus,
    CalendarReminder,
}

impl AlertTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PaneStatus => "pane_status",
            Self::CalendarReminder => "calendar_reminder",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pane_status" => Some(Self::PaneStatus),
            "calendar_reminder" => Some(Self::CalendarReminder),
            _ => None,
        }
    }
}

/// The two states that alert. Every other state never does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertStatus {
    Blocked,
    Done,
}

impl AlertStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "blocked" => Some(Self::Blocked),
            "done" => Some(Self::Done),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertAction {
    Notify,
    Mute,
}

impl AlertAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Notify => "notify",
            Self::Mute => "mute",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "notify" => Some(Self::Notify),
            "mute" => Some(Self::Mute),
            _ => None,
        }
    }
}

/// Who created a rule: the user from the app, or Alfred from a conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleAuthor {
    User,
    Alfred,
}

impl RuleAuthor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Alfred => "alfred",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "user" => Some(Self::User),
            "alfred" => Some(Self::Alfred),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertRule {
    pub id: String,
    pub trigger: AlertTrigger,
    /// `None` names no agent, which matches any agent.
    pub agent: Option<String>,
    /// Only a `pane_status` rule has one; a reminder rule has none.
    pub status: Option<AlertStatus>,
    pub action: AlertAction,
    pub enabled: bool,
    pub author: RuleAuthor,
    pub created_at: String,
}

impl AlertRule {
    /// The rule as REST and Alfred's tools both show it.
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "trigger": self.trigger.as_str(),
            "agent": self.agent,
            "status": self.status.map(AlertStatus::as_str),
            "action": self.action.as_str(),
            "enabled": self.enabled,
            "created_by": self.author.as_str(),
            "created_at": self.created_at,
        })
    }
}

/// What a refused rule field says, the same on REST and in Alfred's tools.
pub fn field_refusal(field: &str) -> String {
    format!("the value of {field} is not allowed here")
}

/// A rule as it is asked for, checked, before the store gives it an id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewAlertRule {
    pub trigger: AlertTrigger,
    pub agent: Option<String>,
    pub status: Option<AlertStatus>,
    pub action: AlertAction,
}

impl NewAlertRule {
    /// Checks the raw fields. The error names the field that is wrong. A `pane_status`
    /// rule needs a status; a `calendar_reminder` rule names neither an agent nor a status.
    pub fn new(
        trigger: &str,
        agent: Option<&str>,
        status: Option<&str>,
        action: &str,
    ) -> Result<Self, &'static str> {
        let trigger = AlertTrigger::parse(trigger).ok_or("trigger")?;
        let action = AlertAction::parse(action).ok_or("action")?;
        let status = status
            .map(|raw| AlertStatus::parse(raw).ok_or("status"))
            .transpose()?;
        let agent = agent
            .map(|raw| agent_name(raw).map_err(|_| "agent"))
            .transpose()?;
        match trigger {
            AlertTrigger::PaneStatus if status.is_none() => return Err("status"),
            AlertTrigger::CalendarReminder if status.is_some() => return Err("status"),
            AlertTrigger::CalendarReminder if agent.is_some() => return Err("agent"),
            _ => {}
        }
        Ok(Self {
            trigger,
            agent,
            status,
            action,
        })
    }
}

/// What the evaluator is asked about: a pane that changed to a status, or a reminder that
/// was delivered. `agent` is the name the pane's seat has, or `None` when it has none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlertSubject<'a> {
    Pane {
        agent: Option<&'a str>,
        status: AlertStatus,
    },
    CalendarReminder,
}

/// The verdict. `Alert` says the change may reach the phone; `rule_id` names the rule that
/// let it through, or `None` when the mode alone did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlertDecision {
    Silent,
    Alert { rule_id: Option<String> },
}

/// Decides one subject. In `all` mode a pane change alerts unless it is muted; in
/// `rules_only` mode, and for every reminder, it alerts only through a `notify` rule.
/// Frames for a pane alert go on through the brakes of `alert_policy`, which the caller applies.
pub fn decide(mode: AlertMode, rules: &[AlertRule], subject: AlertSubject<'_>) -> AlertDecision {
    let matching = |action: AlertAction| {
        rules
            .iter()
            .find(|rule| rule.enabled && rule.action == action && rule.matches(&subject))
    };
    if matching(AlertAction::Mute).is_some() {
        return AlertDecision::Silent;
    }
    let notify = matching(AlertAction::Notify);
    match (&subject, mode, notify) {
        (AlertSubject::Pane { .. }, AlertMode::All, _) => AlertDecision::Alert {
            rule_id: notify.map(|rule| rule.id.clone()),
        },
        (_, _, Some(rule)) => AlertDecision::Alert {
            rule_id: Some(rule.id.clone()),
        },
        (_, _, None) => AlertDecision::Silent,
    }
}

impl AlertRule {
    fn matches(&self, subject: &AlertSubject<'_>) -> bool {
        match subject {
            AlertSubject::CalendarReminder => self.trigger == AlertTrigger::CalendarReminder,
            AlertSubject::Pane { agent, status } => {
                self.trigger == AlertTrigger::PaneStatus
                    && self.status == Some(*status)
                    && self
                        .agent
                        .as_deref()
                        .is_none_or(|named| *agent == Some(named))
            }
        }
    }
}

#[cfg(test)]
#[path = "model_alert_tests.rs"]
mod tests;

#[cfg(test)]
mod validation {
    use super::*;

    #[test]
    fn a_pane_rule_needs_a_status_and_a_reminder_rule_names_none() {
        assert_eq!(
            NewAlertRule::new("pane_status", None, None, "notify"),
            Err("status")
        );
        assert_eq!(
            NewAlertRule::new("calendar_reminder", None, Some("done"), "notify"),
            Err("status")
        );
        assert_eq!(
            NewAlertRule::new("calendar_reminder", Some("bob"), None, "notify"),
            Err("agent")
        );
        assert!(NewAlertRule::new("calendar_reminder", None, None, "mute").is_ok());
    }

    #[test]
    fn unknown_words_name_their_field() {
        assert_eq!(
            NewAlertRule::new("email", None, None, "notify"),
            Err("trigger")
        );
        assert_eq!(
            NewAlertRule::new("pane_status", None, Some("idle"), "notify"),
            Err("status")
        );
        assert_eq!(
            NewAlertRule::new("pane_status", None, Some("done"), "snooze"),
            Err("action")
        );
        assert_eq!(
            NewAlertRule::new("pane_status", Some("   "), Some("done"), "notify"),
            Err("agent")
        );
    }
}
