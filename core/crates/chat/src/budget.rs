//! The active-time budget shared by every wait that pauses on operator approvals:
//! a seat's turn and a delegation's watcher. The time the CLI works, not the time
//! an operator takes to decide one of its approvals. Monotonic clock only; a pause
//! keeps the balance, so a resumed wait never starts again from the full budget.

use std::time::{Duration, Instant};

/// The budget of active time. While the wait is paused by a pending approval of its
/// own, the clock stops, and it runs again when the last wait ends.
pub struct ActiveBudget {
    left: Duration,
    /// When the work started running again; `None` while the wait is paused.
    running_since: Option<Instant>,
}

impl ActiveBudget {
    pub fn new(now: Instant, total: Duration) -> Self {
        Self {
            left: total,
            running_since: Some(now),
        }
    }

    /// Pause while `waiting`, run again when it stops. The same state twice
    /// changes nothing, so overlapping waits count their union once.
    pub fn set_waiting(&mut self, now: Instant, waiting: bool) {
        match (waiting, self.running_since) {
            (true, Some(since)) => {
                self.left = self
                    .left
                    .saturating_sub(now.saturating_duration_since(since));
                self.running_since = None;
            }
            (false, None) => self.running_since = Some(now),
            _ => {}
        }
    }

    /// When the budget runs out; `None` while the wait is paused.
    pub fn deadline(&self) -> Option<Instant> {
        self.running_since.map(|since| since + self.left)
    }

    /// The budget ran out while running. Nothing that comes later (a card opened
    /// after the end of the active time) can pause it again.
    pub fn is_exhausted(&self, now: Instant) -> bool {
        self.deadline().is_some_and(|at| at <= now)
    }
}

/// The frames that can start, end or move a wait: an approval's transitions and
/// an archive of its conversation (`chat.approval`, `chat.conversation`).
pub fn is_wait_frame(event: &str) -> bool {
    matches!(event, "chat.approval" | "chat.conversation")
}

/// Sleeps until `at` on the monotonic clock; without an instant it never wakes.
pub async fn until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await,
        None => std::future::pending::<()>().await,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{is_wait_frame, ActiveBudget};

    fn at(start: Instant, seconds: u64) -> Instant {
        start + Duration::from_secs(seconds)
    }

    #[test]
    fn a_wait_without_approvals_runs_out_at_the_budget() {
        let start = Instant::now();
        let budget = ActiveBudget::new(start, Duration::from_secs(600));
        assert_eq!(budget.deadline(), Some(at(start, 600)));
    }

    #[test]
    fn an_approval_longer_than_the_budget_does_not_end_the_wait() {
        let start = Instant::now();
        let mut budget = ActiveBudget::new(start, Duration::from_secs(600));
        budget.set_waiting(at(start, 10), true);
        assert_eq!(budget.deadline(), None, "a paused wait has no deadline");
        // Decided after 890 s of waiting: 590 s of budget are left.
        budget.set_waiting(at(start, 900), false);
        assert_eq!(budget.deadline(), Some(at(start, 900 + 590)));
    }

    #[test]
    fn an_expired_approval_resumes_with_the_balance_it_had() {
        let start = Instant::now();
        let mut budget = ActiveBudget::new(start, Duration::from_secs(600));
        budget.set_waiting(at(start, 100), true);
        budget.set_waiting(at(start, 220), false);
        // 100 s ran before the wait: 500 s are left, not a fresh 600.
        assert_eq!(budget.deadline(), Some(at(start, 220 + 500)));
    }

    #[test]
    fn overlapping_waits_discount_their_union_once() {
        // Two approvals overlap on [50, 100]; the union is [0, 150], not 200 s.
        let start = Instant::now();
        let mut budget = ActiveBudget::new(start, Duration::from_secs(600));
        budget.set_waiting(at(start, 0), true);
        budget.set_waiting(at(start, 50), true);
        budget.set_waiting(at(start, 100), true);
        budget.set_waiting(at(start, 150), false);
        assert_eq!(budget.deadline(), Some(at(start, 150 + 600)));
    }

    #[test]
    fn a_balance_that_ran_out_while_running_stays_out() {
        let start = Instant::now();
        let budget = ActiveBudget::new(start, Duration::from_secs(600));
        assert!(!budget.is_exhausted(at(start, 599)));
        assert!(budget.is_exhausted(at(start, 600)));
        // A paused wait has no deadline, so it is not exhausted by definition.
        let mut paused = ActiveBudget::new(start, Duration::from_secs(600));
        paused.set_waiting(at(start, 10), true);
        assert!(!paused.is_exhausted(at(start, 5000)));
    }

    #[test]
    fn repeating_the_same_state_changes_nothing() {
        let start = Instant::now();
        let mut budget = ActiveBudget::new(start, Duration::from_secs(600));
        budget.set_waiting(at(start, 10), true);
        budget.set_waiting(at(start, 20), true);
        budget.set_waiting(at(start, 30), false);
        budget.set_waiting(at(start, 40), false);
        // Only the first 10 s ran: 590 s are left from t = 30.
        assert_eq!(budget.deadline(), Some(at(start, 30 + 590)));
    }

    #[test]
    fn only_approval_and_conversation_frames_move_a_wait() {
        assert!(is_wait_frame("chat.approval"));
        assert!(is_wait_frame("chat.conversation"));
        assert!(!is_wait_frame("chat.message"));
        assert!(!is_wait_frame("chat.delegation"));
    }
}
