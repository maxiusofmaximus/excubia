//! The wire shapes of the alert rules (`/v1/alerts`): what a rule says, the draft that makes
//! one, and the mode the host runs them in. The screen and Alfred read the same words.

use serde::{Deserialize, Serialize};

/// What a rule listens to: an agent's pane changing, or a reminder of the calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertTrigger {
    PaneStatus,
    CalendarReminder,
}

/// The pane state a `pane_status` rule waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertStatus {
    Blocked,
    Done,
}

/// What a matching change does. `mute` wins over any `notify`, in every mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertAction {
    Notify,
    Mute,
}

/// `all` warns about every block and every finish; `rules_only` only about what a
/// `notify` rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertMode {
    All,
    RulesOnly,
}

/// A rule as the host keeps it. `agent: None` is any agent; `status` is absent for a
/// calendar reminder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: String,
    pub trigger: AlertTrigger,
    pub agent: Option<String>,
    pub status: Option<AlertStatus>,
    pub action: AlertAction,
    pub enabled: bool,
    pub created_by: String,
    pub created_at: String,
}

/// What the screen sends to make a rule. A calendar reminder has no `status`, which the
/// host refuses if it is there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertRuleDraft {
    pub trigger: AlertTrigger,
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AlertStatus>,
    pub action: AlertAction,
}
