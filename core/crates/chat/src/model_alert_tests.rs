use super::{
    decide, AlertAction, AlertDecision, AlertMode, AlertRule, AlertStatus, AlertSubject,
    AlertTrigger, RuleAuthor,
};

fn rule(
    id: &str,
    action: AlertAction,
    agent: Option<&str>,
    status: Option<AlertStatus>,
) -> AlertRule {
    let trigger = if status.is_some() {
        AlertTrigger::PaneStatus
    } else {
        AlertTrigger::CalendarReminder
    };
    AlertRule {
        id: id.to_string(),
        trigger,
        agent: agent.map(str::to_string),
        status,
        action,
        enabled: true,
        author: RuleAuthor::User,
        created_at: "2026-10-29T00:00:00.000Z".to_string(),
    }
}

fn pane(agent: Option<&str>, status: AlertStatus) -> AlertSubject<'_> {
    AlertSubject::Pane { agent, status }
}

fn alert(rule_id: Option<&str>) -> AlertDecision {
    AlertDecision::Alert {
        rule_id: rule_id.map(str::to_string),
    }
}

#[test]
fn with_no_rules_every_agent_stays_as_it_was() {
    let rules: [AlertRule; 0] = [];
    assert_eq!(
        decide(AlertMode::All, &rules, pane(Some("bob"), AlertStatus::Done)),
        alert(None)
    );
    assert_eq!(
        decide(AlertMode::All, &rules, pane(None, AlertStatus::Blocked)),
        alert(None)
    );
}

#[test]
fn rules_only_with_no_matching_rule_stays_silent() {
    let rules = [rule(
        "r1",
        AlertAction::Notify,
        Some("ana"),
        Some(AlertStatus::Done),
    )];
    assert_eq!(
        decide(
            AlertMode::RulesOnly,
            &rules,
            pane(Some("bob"), AlertStatus::Done)
        ),
        AlertDecision::Silent
    );
    assert_eq!(
        decide(
            AlertMode::RulesOnly,
            &[],
            pane(Some("bob"), AlertStatus::Blocked)
        ),
        AlertDecision::Silent
    );
}

#[test]
fn a_matching_notify_names_its_rule_in_either_mode() {
    let rules = [rule(
        "r1",
        AlertAction::Notify,
        Some("bob"),
        Some(AlertStatus::Done),
    )];
    assert_eq!(
        decide(
            AlertMode::RulesOnly,
            &rules,
            pane(Some("bob"), AlertStatus::Done)
        ),
        alert(Some("r1"))
    );
    assert_eq!(
        decide(AlertMode::All, &rules, pane(Some("bob"), AlertStatus::Done)),
        alert(Some("r1"))
    );
}

#[test]
fn mute_wins_over_notify_in_every_mode() {
    let rules = [
        rule("n1", AlertAction::Notify, None, Some(AlertStatus::Done)),
        rule(
            "m1",
            AlertAction::Mute,
            Some("bob"),
            Some(AlertStatus::Done),
        ),
    ];
    for mode in [AlertMode::All, AlertMode::RulesOnly] {
        assert_eq!(
            decide(mode, &rules, pane(Some("bob"), AlertStatus::Done)),
            AlertDecision::Silent
        );
    }
    assert_eq!(
        decide(AlertMode::All, &rules, pane(Some("ana"), AlertStatus::Done)),
        alert(Some("n1"))
    );
}

#[test]
fn a_mute_silences_even_all_mode_without_a_notify() {
    let rules = [rule(
        "m1",
        AlertAction::Mute,
        Some("bob"),
        Some(AlertStatus::Blocked),
    )];
    assert_eq!(
        decide(
            AlertMode::All,
            &rules,
            pane(Some("bob"), AlertStatus::Blocked)
        ),
        AlertDecision::Silent
    );
    assert_eq!(
        decide(
            AlertMode::All,
            &rules,
            pane(Some("ana"), AlertStatus::Blocked)
        ),
        alert(None)
    );
}

#[test]
fn a_named_agent_matches_only_that_agent() {
    let rules = [rule(
        "r1",
        AlertAction::Notify,
        Some("bob"),
        Some(AlertStatus::Done),
    )];
    assert_eq!(
        decide(
            AlertMode::RulesOnly,
            &rules,
            pane(Some("ana"), AlertStatus::Done)
        ),
        AlertDecision::Silent
    );
}

/// A pane whose name cannot be resolved matches only the rules that name no agent.
#[test]
fn an_unresolved_agent_matches_only_the_rules_for_any_agent() {
    let named = [rule(
        "r1",
        AlertAction::Notify,
        Some("bob"),
        Some(AlertStatus::Done),
    )];
    assert_eq!(
        decide(AlertMode::RulesOnly, &named, pane(None, AlertStatus::Done)),
        AlertDecision::Silent
    );
    let any = [rule(
        "r2",
        AlertAction::Notify,
        None,
        Some(AlertStatus::Done),
    )];
    assert_eq!(
        decide(AlertMode::RulesOnly, &any, pane(None, AlertStatus::Done)),
        alert(Some("r2"))
    );
}

#[test]
fn a_status_filter_must_match_the_status() {
    let rules = [rule(
        "r1",
        AlertAction::Notify,
        None,
        Some(AlertStatus::Blocked),
    )];
    assert_eq!(
        decide(
            AlertMode::RulesOnly,
            &rules,
            pane(Some("bob"), AlertStatus::Done)
        ),
        AlertDecision::Silent
    );
}

#[test]
fn a_disabled_rule_is_ignored() {
    let mut mute = rule("m1", AlertAction::Mute, None, Some(AlertStatus::Done));
    mute.enabled = false;
    let mut notify = rule("n1", AlertAction::Notify, None, Some(AlertStatus::Done));
    notify.enabled = false;
    assert_eq!(
        decide(
            AlertMode::All,
            &[mute],
            pane(Some("bob"), AlertStatus::Done)
        ),
        alert(None)
    );
    assert_eq!(
        decide(
            AlertMode::RulesOnly,
            &[notify],
            pane(Some("bob"), AlertStatus::Done)
        ),
        AlertDecision::Silent
    );
}

#[test]
fn a_reminder_needs_a_notify_rule_whatever_the_mode() {
    for mode in [AlertMode::All, AlertMode::RulesOnly] {
        assert_eq!(
            decide(mode, &[], AlertSubject::CalendarReminder),
            AlertDecision::Silent
        );
    }
    let rules = [rule("r1", AlertAction::Notify, None, None)];
    assert_eq!(
        decide(AlertMode::All, &rules, AlertSubject::CalendarReminder),
        alert(Some("r1"))
    );
}

#[test]
fn a_reminder_mute_silences_its_live_frame() {
    let rules = [
        rule("n1", AlertAction::Notify, None, None),
        rule("m1", AlertAction::Mute, None, None),
    ];
    assert_eq!(
        decide(AlertMode::All, &rules, AlertSubject::CalendarReminder),
        AlertDecision::Silent
    );
}

#[test]
fn a_pane_rule_never_matches_a_reminder_and_the_reverse() {
    let pane_rules = [rule(
        "r1",
        AlertAction::Notify,
        None,
        Some(AlertStatus::Done),
    )];
    assert_eq!(
        decide(
            AlertMode::RulesOnly,
            &pane_rules,
            AlertSubject::CalendarReminder
        ),
        AlertDecision::Silent
    );
    let reminder = [rule("r2", AlertAction::Notify, None, None)];
    assert_eq!(
        decide(
            AlertMode::RulesOnly,
            &reminder,
            pane(Some("bob"), AlertStatus::Done)
        ),
        AlertDecision::Silent
    );
}
