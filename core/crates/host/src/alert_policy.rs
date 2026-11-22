//! When a state change becomes an alert. Port of the first implementation's
//! `policy.py` (contract: `api.md`, "Reglas de aviso"). Three brakes, checked
//! in order: eligibility, cooldown, budget. Quiet hours defer `done` only.

use std::collections::{HashMap, VecDeque};

use excubia_state::Status;

pub const HOUR_SECS: f64 = 3600.0;
pub const DAY_SECS: f64 = 86_400.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Defer,
    Suppress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Eligible,
    Synthetic,
    Removed,
    NotNotifiable,
    Cooldown,
    Budget,
    QuietHours,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decision {
    pub verdict: Verdict,
    pub reason: Reason,
    /// Unix seconds when a deferred or cooled-down change may be looked at again.
    pub wake_at: Option<f64>,
}

impl Decision {
    fn suppress(reason: Reason) -> Self {
        Self {
            verdict: Verdict::Suppress,
            reason,
            wake_at: None,
        }
    }
}

/// A state change, as the alert judge sees it.
#[derive(Clone, Copy, Debug)]
pub struct Change<'a> {
    pub pane_id: &'a str,
    pub status: Status,
    /// Came from a snapshot (startup or recovery), not from a live event.
    pub synthetic: bool,
    pub removed: bool,
}

/// An instant: Unix seconds and the local UTC offset. The offset comes from the
/// caller, so quiet hours need no clock of their own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moment {
    pub unix_secs: f64,
    pub utc_offset_secs: i32,
}

/// Local hours `start..end` in 0..=23, wrapping past midnight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuietHours {
    start: u8,
    end: u8,
}

impl QuietHours {
    pub fn new(start: u8, end: u8) -> Option<Self> {
        (start < 24 && end < 24).then_some(Self { start, end })
    }

    fn contains(self, hour: u8) -> bool {
        if self.start < self.end {
            (self.start..self.end).contains(&hour)
        } else {
            hour >= self.start || hour < self.end
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyConfig {
    /// Minimum gap between two notices for the same agent.
    pub cooldown_secs: f64,
    /// Repeat window for an agent still blocked. `None` disables the repeat.
    pub escalate_after_secs: Option<f64>,
    /// Budgets of zero are disabled.
    pub max_per_agent_per_hour: usize,
    pub max_per_hour: usize,
    pub max_per_day: usize,
    pub quiet_hours: Option<QuietHours>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            cooldown_secs: 120.0,
            escalate_after_secs: Some(900.0),
            max_per_agent_per_hour: 6,
            max_per_hour: 60,
            max_per_day: 400,
            quiet_hours: None,
        }
    }
}

/// `blocked` waits for a human decision; `done` is worth a look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Attention {
    Urgent,
    Review,
}

fn attention(status: Status) -> Option<Attention> {
    match status {
        Status::Blocked => Some(Attention::Urgent),
        Status::Done => Some(Attention::Review),
        Status::Idle | Status::Working | Status::Unknown => None,
    }
}

pub struct AlertPolicy {
    config: PolicyConfig,
    last_sent: HashMap<String, f64>,
    /// Allowed notices of the last day, oldest first.
    history: VecDeque<(f64, String)>,
}

impl AlertPolicy {
    pub fn new(config: PolicyConfig) -> Self {
        Self {
            config,
            last_sent: HashMap::new(),
            history: VecDeque::new(),
        }
    }

    /// Decides one change. Only an `Allow` is recorded against the budgets.
    pub fn decide(&mut self, change: &Change<'_>, now: Moment) -> Decision {
        if change.synthetic {
            return Decision::suppress(Reason::Synthetic);
        }
        if change.removed {
            return Decision::suppress(Reason::Removed);
        }
        let Some(attention) = attention(change.status) else {
            return Decision::suppress(Reason::NotNotifiable);
        };
        if let Some(deferred) = self.quiet_hours(attention, now) {
            return deferred;
        }
        if !self.past_cooldown(change.pane_id, attention, now.unix_secs) {
            return Decision {
                verdict: Verdict::Suppress,
                reason: Reason::Cooldown,
                wake_at: self
                    .last_sent
                    .get(change.pane_id)
                    .map(|last| last + self.config.cooldown_secs),
            };
        }
        if !self.within_budget(change.pane_id, now.unix_secs) {
            return Decision::suppress(Reason::Budget);
        }
        self.record(change.pane_id, now.unix_secs);
        Decision {
            verdict: Verdict::Allow,
            reason: Reason::Eligible,
            wake_at: None,
        }
    }

    /// Urgent notices ignore quiet hours: an approval that waits until morning
    /// keeps an agent idle for hours.
    fn quiet_hours(&self, attention: Attention, now: Moment) -> Option<Decision> {
        let quiet = self.config.quiet_hours?;
        if attention == Attention::Urgent {
            return None;
        }
        let local = (now.unix_secs + f64::from(now.utc_offset_secs)).rem_euclid(DAY_SECS);
        let hour = (local / HOUR_SECS) as u8;
        if !quiet.contains(hour) {
            return None;
        }
        let until_end = (f64::from(quiet.end) * HOUR_SECS - local).rem_euclid(DAY_SECS);
        let wait = if until_end == 0.0 {
            DAY_SECS
        } else {
            until_end
        };
        Some(Decision {
            verdict: Verdict::Defer,
            reason: Reason::QuietHours,
            wake_at: Some(now.unix_secs + wait),
        })
    }

    fn past_cooldown(&self, pane_id: &str, attention: Attention, now_secs: f64) -> bool {
        let Some(last) = self.last_sent.get(pane_id) else {
            return true;
        };
        let window = match (attention, self.config.escalate_after_secs) {
            (Attention::Urgent, Some(escalate)) => escalate,
            _ => self.config.cooldown_secs,
        };
        now_secs - last >= window
    }

    fn within_budget(&mut self, pane_id: &str, now_secs: f64) -> bool {
        let day_cutoff = now_secs - DAY_SECS;
        while self.history.front().is_some_and(|(at, _)| *at < day_cutoff) {
            self.history.pop_front();
        }
        let hour_cutoff = now_secs - HOUR_SECS;
        let config = &self.config;
        let in_hour = self.sent_since(hour_cutoff, None);
        let agent_in_hour = self.sent_since(hour_cutoff, Some(pane_id));
        (config.max_per_hour == 0 || in_hour < config.max_per_hour)
            && (config.max_per_agent_per_hour == 0 || agent_in_hour < config.max_per_agent_per_hour)
            && (config.max_per_day == 0 || self.history.len() < config.max_per_day)
    }

    fn sent_since(&self, cutoff: f64, pane_id: Option<&str>) -> usize {
        self.history
            .iter()
            .filter(|(at, pane)| *at >= cutoff && pane_id.is_none_or(|id| id == pane))
            .count()
    }

    fn record(&mut self, pane_id: &str, now_secs: f64) {
        self.last_sent.insert(pane_id.to_string(), now_secs);
        self.history.push_back((now_secs, pane_id.to_string()));
    }
}

#[cfg(test)]
mod tests;
