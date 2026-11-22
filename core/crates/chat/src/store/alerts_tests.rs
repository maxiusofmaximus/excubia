use crate::error::ChatError;
use crate::model_alert::{AlertDecision, AlertMode, AlertStatus, NewAlertRule, MAX_ALERT_RULES};
use crate::seat::SeatActor;
use crate::store::alerts::AlertWriter;
use crate::store::ChatStore;

const PANE: &str = "w9-1";

fn store_with_alfred_on_a_pane() -> (ChatStore, SeatActor) {
    let store = ChatStore::in_memory().expect("store");
    let agent_id = store.alfred_id().expect("alfred");
    store.set_seat_pane(&agent_id, Some(PANE)).expect("pane");
    let actor = SeatActor {
        agent_id,
        role_id: "alfred".to_string(),
        project_id: None,
        conversation_id: "conv".to_string(),
        run_id: "run".to_string(),
    };
    (store, actor)
}

fn pane_rule(agent: Option<&str>, status: &str, action: &str) -> NewAlertRule {
    NewAlertRule::new("pane_status", agent, Some(status), action).expect("valid")
}

fn reminder_rule(action: &str) -> NewAlertRule {
    NewAlertRule::new("calendar_reminder", None, None, action).expect("valid")
}

fn audit_lines(store: &ChatStore, object_kind: &str) -> i64 {
    store
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM agenda_audit WHERE object_kind = ?1",
            [object_kind],
            |row| row.get(0),
        )
        .expect("audit count")
}

#[test]
fn the_mode_is_all_until_it_is_set_and_the_change_is_audited() {
    let (store, actor) = store_with_alfred_on_a_pane();
    assert_eq!(store.alert_mode().expect("mode"), AlertMode::All);
    store
        .set_alert_mode(AlertWriter::Seat(&actor), AlertMode::RulesOnly)
        .expect("set");
    assert_eq!(store.alert_mode().expect("mode"), AlertMode::RulesOnly);
    assert_eq!(audit_lines(&store, "alert_mode"), 1);
}

#[test]
fn a_rule_is_kept_listed_and_written_to_the_audit() {
    let (store, actor) = store_with_alfred_on_a_pane();
    let rule = store
        .create_alert_rule(
            AlertWriter::Seat(&actor),
            pane_rule(Some("Alfred"), "done", "notify"),
        )
        .expect("create");
    assert!(rule.id.starts_with("ar_"));
    assert!(rule.enabled);
    let listed = store.alert_rules().expect("list");
    assert_eq!(listed, vec![rule]);
    assert_eq!(audit_lines(&store, "alert_rule"), 1);
}

#[test]
fn an_exact_duplicate_is_refused_and_a_different_action_is_not() {
    let (store, _) = store_with_alfred_on_a_pane();
    store
        .create_alert_rule(
            AlertWriter::Device("dev-1"),
            pane_rule(None, "done", "notify"),
        )
        .expect("first");
    assert!(matches!(
        store.create_alert_rule(
            AlertWriter::Device("dev-1"),
            pane_rule(None, "done", "notify")
        ),
        Err(ChatError::AlertRuleExists)
    ));
    store
        .create_alert_rule(
            AlertWriter::Device("dev-1"),
            pane_rule(None, "done", "mute"),
        )
        .expect("another action is another rule");
    store
        .create_alert_rule(AlertWriter::Device("dev-1"), reminder_rule("notify"))
        .expect("a reminder rule has no status to clash with");
    assert!(matches!(
        store.create_alert_rule(AlertWriter::Device("dev-1"), reminder_rule("notify")),
        Err(ChatError::AlertRuleExists)
    ));
}

#[test]
fn a_company_keeps_at_most_one_hundred_rules() {
    let (store, _) = store_with_alfred_on_a_pane();
    for index in 0..MAX_ALERT_RULES {
        let name = format!("agente {index}");
        store
            .create_alert_rule(
                AlertWriter::Device("dev-1"),
                pane_rule(Some(&name), "done", "notify"),
            )
            .expect("within the limit");
    }
    let next = store.create_alert_rule(
        AlertWriter::Device("dev-1"),
        pane_rule(Some("otro"), "done", "notify"),
    );
    assert!(matches!(next, Err(ChatError::AlertRuleLimit)));
    assert_eq!(store.alert_rules().expect("list").len(), 100);
}

#[test]
fn a_disabled_rule_stops_deciding_and_an_unknown_one_is_not_found() {
    let (store, _) = store_with_alfred_on_a_pane();
    let rule = store
        .create_alert_rule(AlertWriter::Device("dev-1"), reminder_rule("notify"))
        .expect("create");
    assert!(matches!(
        store.alert_for_reminder().expect("decide"),
        AlertDecision::Alert { .. }
    ));
    let off = store
        .set_alert_rule_enabled(AlertWriter::Device("dev-1"), &rule.id, false)
        .expect("disable");
    assert!(!off.enabled);
    assert_eq!(
        store.alert_for_reminder().expect("decide"),
        AlertDecision::Silent
    );
    assert!(matches!(
        store.set_alert_rule_enabled(AlertWriter::Device("dev-1"), "ar_nope", true),
        Err(ChatError::AlertRuleNotFound(_))
    ));
}

#[test]
fn a_deleted_rule_is_gone_and_deleting_it_again_is_not_found() {
    let (store, _) = store_with_alfred_on_a_pane();
    let rule = store
        .create_alert_rule(AlertWriter::Device("dev-1"), reminder_rule("mute"))
        .expect("create");
    store
        .delete_alert_rule(AlertWriter::Device("dev-1"), &rule.id)
        .expect("delete");
    assert!(store.alert_rules().expect("list").is_empty());
    assert!(matches!(
        store.delete_alert_rule(AlertWriter::Device("dev-1"), &rule.id),
        Err(ChatError::AlertRuleNotFound(_))
    ));
}

/// The pane's agent is found by its seat's name, as the chat shows it.
#[test]
fn a_pane_alert_names_its_agent_from_the_seat_and_a_rule_for_it_matches() {
    let (store, actor) = store_with_alfred_on_a_pane();
    store
        .set_alert_mode(AlertWriter::Seat(&actor), AlertMode::RulesOnly)
        .expect("rules only");
    assert_eq!(
        store
            .alert_for_pane(PANE, AlertStatus::Done)
            .expect("decide"),
        AlertDecision::Silent
    );
    let rule = store
        .create_alert_rule(
            AlertWriter::Seat(&actor),
            pane_rule(Some("Alfred"), "done", "notify"),
        )
        .expect("create");
    assert_eq!(
        store
            .alert_for_pane(PANE, AlertStatus::Done)
            .expect("decide"),
        AlertDecision::Alert {
            rule_id: Some(rule.id)
        }
    );
    assert_eq!(
        store
            .alert_for_pane("w9-2", AlertStatus::Done)
            .expect("decide"),
        AlertDecision::Silent,
        "a pane with no seat matches no named agent"
    );
}

#[test]
fn a_mute_on_a_pane_wins_over_the_mode_all() {
    let (store, _) = store_with_alfred_on_a_pane();
    store
        .create_alert_rule(
            AlertWriter::Device("dev-1"),
            pane_rule(None, "blocked", "mute"),
        )
        .expect("mute");
    assert_eq!(
        store
            .alert_for_pane(PANE, AlertStatus::Blocked)
            .expect("decide"),
        AlertDecision::Silent
    );
    assert_eq!(
        store
            .alert_for_pane(PANE, AlertStatus::Done)
            .expect("decide"),
        AlertDecision::Alert { rule_id: None }
    );
}

#[test]
fn a_summary_counts_only_the_enabled_rules() {
    let (store, _) = store_with_alfred_on_a_pane();
    let rule = store
        .create_alert_rule(AlertWriter::Device("dev-1"), reminder_rule("notify"))
        .expect("create");
    store
        .create_alert_rule(
            AlertWriter::Device("dev-1"),
            pane_rule(None, "done", "mute"),
        )
        .expect("create");
    store
        .set_alert_rule_enabled(AlertWriter::Device("dev-1"), &rule.id, false)
        .expect("disable");
    let summary = store.alert_summary().expect("summary");
    assert_eq!(summary.mode, AlertMode::All);
    assert_eq!(summary.enabled_count, 1);
}
