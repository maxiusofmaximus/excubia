use super::*;

fn at(unix_secs: f64) -> Moment {
    Moment {
        unix_secs,
        utc_offset_secs: 0,
    }
}

fn change(pane_id: &str, status: Status) -> Change<'_> {
    Change {
        pane_id,
        status,
        synthetic: false,
        removed: false,
    }
}

fn policy() -> AlertPolicy {
    AlertPolicy::new(PolicyConfig::default())
}

#[test]
fn only_blocked_and_done_ever_alert() {
    let mut policy = policy();
    for status in [Status::Working, Status::Idle, Status::Unknown] {
        let decision = policy.decide(&change("p1", status), at(0.0));
        assert_eq!(decision.reason, Reason::NotNotifiable);
    }
    assert_eq!(
        policy.decide(&change("p1", Status::Done), at(0.0)).verdict,
        Verdict::Allow
    );
}

#[test]
fn snapshots_and_removals_never_alert() {
    let mut policy = policy();
    let synthetic = Change {
        synthetic: true,
        ..change("p1", Status::Blocked)
    };
    let removed = Change {
        removed: true,
        ..change("p1", Status::Blocked)
    };
    assert_eq!(policy.decide(&synthetic, at(0.0)).reason, Reason::Synthetic);
    assert_eq!(policy.decide(&removed, at(0.0)).reason, Reason::Removed);
}

#[test]
fn a_second_notice_waits_out_the_cooldown() {
    let mut policy = policy();
    assert_eq!(
        policy.decide(&change("p1", Status::Done), at(0.0)).verdict,
        Verdict::Allow
    );
    let early = policy.decide(&change("p1", Status::Done), at(119.0));
    assert_eq!(early.reason, Reason::Cooldown);
    assert_eq!(early.wake_at, Some(120.0));
    assert_eq!(
        policy
            .decide(&change("p1", Status::Done), at(120.0))
            .verdict,
        Verdict::Allow
    );
}

#[test]
fn a_still_blocked_agent_repeats_after_the_escalation_window() {
    let mut policy = policy();
    assert_eq!(
        policy
            .decide(&change("p1", Status::Blocked), at(0.0))
            .verdict,
        Verdict::Allow
    );
    assert_eq!(
        policy
            .decide(&change("p1", Status::Blocked), at(600.0))
            .reason,
        Reason::Cooldown
    );
    assert_eq!(
        policy
            .decide(&change("p1", Status::Blocked), at(900.0))
            .verdict,
        Verdict::Allow
    );
}

#[test]
fn an_agent_is_limited_to_six_notices_an_hour() {
    let mut policy = policy();
    for n in 0..6 {
        let now = at(f64::from(n) * 130.0);
        assert_eq!(
            policy.decide(&change("p1", Status::Done), now).verdict,
            Verdict::Allow
        );
    }
    let blocked = policy.decide(&change("p1", Status::Done), at(6.0 * 130.0));
    assert_eq!(blocked.reason, Reason::Budget);
    assert_eq!(
        policy
            .decide(&change("p2", Status::Done), at(6.0 * 130.0))
            .verdict,
        Verdict::Allow
    );
}

#[test]
fn the_global_hourly_budget_counts_every_agent() {
    let config = PolicyConfig {
        max_per_agent_per_hour: 0,
        max_per_hour: 2,
        ..PolicyConfig::default()
    };
    let mut policy = AlertPolicy::new(config);
    assert_eq!(
        policy.decide(&change("a", Status::Done), at(0.0)).verdict,
        Verdict::Allow
    );
    assert_eq!(
        policy.decide(&change("b", Status::Done), at(1.0)).verdict,
        Verdict::Allow
    );
    assert_eq!(
        policy.decide(&change("c", Status::Done), at(2.0)).reason,
        Reason::Budget
    );
    assert_eq!(
        policy
            .decide(&change("c", Status::Done), at(HOUR_SECS + 2.0))
            .verdict,
        Verdict::Allow
    );
}

#[test]
fn the_daily_budget_stops_a_long_run() {
    let config = PolicyConfig {
        max_per_agent_per_hour: 0,
        max_per_hour: 0,
        max_per_day: 2,
        ..PolicyConfig::default()
    };
    let mut policy = AlertPolicy::new(config);
    assert_eq!(
        policy.decide(&change("a", Status::Done), at(0.0)).verdict,
        Verdict::Allow
    );
    assert_eq!(
        policy.decide(&change("b", Status::Done), at(1.0)).verdict,
        Verdict::Allow
    );
    assert_eq!(
        policy.decide(&change("c", Status::Done), at(2.0)).reason,
        Reason::Budget
    );
}

#[test]
fn quiet_hours_defer_done_until_the_end_hour_and_never_block() {
    let config = PolicyConfig {
        quiet_hours: QuietHours::new(22, 7),
        ..PolicyConfig::default()
    };
    let mut policy = AlertPolicy::new(config);
    // 23:00 local (UTC+0): inside 22..07.
    let night = Moment {
        unix_secs: 23.0 * HOUR_SECS,
        utc_offset_secs: 0,
    };
    let deferred = policy.decide(&change("a", Status::Done), night);
    assert_eq!(deferred.verdict, Verdict::Defer);
    assert_eq!(deferred.wake_at, Some(31.0 * HOUR_SECS));
    assert_eq!(
        policy.decide(&change("a", Status::Blocked), night).verdict,
        Verdict::Allow
    );
}

#[test]
fn quiet_hours_membership_wraps_past_midnight() {
    let quiet = QuietHours::new(22, 7).expect("valid hours");
    assert!(quiet.contains(23));
    assert!(!quiet.contains(12));
    let day = QuietHours::new(9, 17).expect("valid hours");
    assert!(day.contains(9) && !day.contains(17));
    assert!(QuietHours::new(24, 1).is_none());
}
