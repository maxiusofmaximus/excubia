//! Manifest engine tests.
use super::*;

#[test]
fn all_bundled_manifests_compile() {
    for id in ["claude", "codex", "opencode", "fake"] {
        let detector = Detector::bundled(id).expect("bundled");
        assert_eq!(detector.id, id);
        assert!(!detector.rules.is_empty());
    }
}

// Adapted from herdr's `rule_semantics_apply_gates_priority_and_line_regex`
// (Apache-2.0): priority order, nested gates, `not`, and line_regex.
#[test]
fn rule_semantics_apply_gates_priority_and_line_regex() {
    let detector = Detector::from_toml(
        r#"
id = "semantics"
[[rules]]
id = "low_contains"
state = "idle"
priority = 1
region = "whole_recent"
contains = ["match"]

[[rules]]
id = "high_nested_gates"
state = "working"
priority = 10
region = "whole_recent"
contains = ["match"]
all = [
  { any = [{ regex = ["w[io]n"] }, { contains = ["fallback"] }] },
]
not = [
  { contains = ["blocked"] },
]

[[rules]]
id = "line_regex"
state = "blocked"
priority = 20
region = "whole_recent"
line_regex = ["^exact line$"]
"#,
    )
    .expect("manifest");

    let high = detector.detect("match win", "").expect("high rule");
    assert_eq!(high.state, DetectedState::Working);
    assert_eq!(high.rule_id, "high_nested_gates");

    let not_gate = detector.detect("match win blocked", "").expect("low rule");
    assert_eq!(not_gate.state, DetectedState::Idle);
    assert_eq!(not_gate.rule_id, "low_contains");

    let line = detector
        .detect("before\nexact line\nafter", "")
        .expect("line rule");
    assert_eq!(line.state, DetectedState::Blocked);
    assert_eq!(line.rule_id, "line_regex");
}

#[test]
fn contains_is_case_insensitive_like_herdr() {
    let detector = Detector::from_toml(
        r#"
id = "case"
[[rules]]
id = "blocker"
state = "blocked"
priority = 10
region = "whole_recent"
contains = ["esc to cancel"]
"#,
    )
    .expect("manifest");
    assert!(detector
        .detect("  Esc to cancel · Enter to confirm", "")
        .is_some());
    assert!(detector.detect("nothing here", "").is_none());
}

#[test]
fn after_last_horizontal_rule_isolates_the_live_dialog() {
    let detector = Detector::from_toml(
        r#"
id = "rule-region"
[[rules]]
id = "blocker"
state = "blocked"
priority = 10
region = "after_last_horizontal_rule"
contains = ["esc to cancel"]
"#,
    )
    .expect("manifest");

    let screen = "old question: do you want to proceed?\n───────────\nDone.\n";
    assert!(detector.detect(screen, "").is_none());

    let live = "old output\n───────────\n❯ 1. Yes\n  Esc to cancel · Enter to confirm\n";
    assert_eq!(
        detector.detect(live, "").expect("live dialog").state,
        DetectedState::Blocked
    );
}

#[test]
fn osc_title_rules_use_the_title_input() {
    let detector = Detector::from_toml(
        r#"
id = "title"
[[rules]]
id = "needs_action"
state = "blocked"
priority = 10
region = "osc_title"
contains = ["action required"]
"#,
    )
    .expect("manifest");
    assert!(detector.detect("", "Action Required").is_some());
    assert!(detector.detect("Action Required", "").is_none());
}

#[test]
fn skip_state_update_reports_no_update() {
    let detector = Detector::from_toml(
        r#"
id = "skip"
[[rules]]
id = "viewer"
state = "unknown"
priority = 10
region = "whole_recent"
skip_state_update = true
contains = ["showing detailed transcript"]
"#,
    )
    .expect("manifest");
    assert!(detector.detect("showing detailed transcript", "").is_none());
}

#[test]
fn fake_agent_approval_screen_is_blocked() {
    let detector = Detector::bundled("fake").expect("bundled");
    let screen = "Trabajando en la tarea...\n\n╭──────────────────────────────────────────╮\n│ ¿Quieres continuar?                      │\n│ ❯ 1. Sí                                  │\n│   2. No                                  │\n╰──────────────────────────────────────────╯\n────────────────────────────────────────────\n  Esc to cancel · Enter to confirm";
    let detection = detector.detect(screen, "").expect("blocked");
    assert_eq!(detection.state, DetectedState::Blocked);
    assert_eq!(detection.rule_id, "approval_blocked");
}

#[test]
fn fake_agent_prompt_after_finishing_is_idle() {
    let detector = Detector::bundled("fake").expect("bundled");
    let screen = "Trabajando en la tarea...\n\n╭──────────────────────────────────────────╮\n│ ¿Quieres continuar?                      │\n│ ❯ 1. Sí                                  │\n│   2. No                                  │\n╰──────────────────────────────────────────╯\n────────────────────────────────────────────\n  Esc to cancel · Enter to confirm\ny\nRECIBIDO: y\nterminado\n────────────────────────────────────────────\n❯ ";
    let detection = detector.detect(screen, "").expect("idle");
    assert_eq!(detection.state, DetectedState::Idle);
    assert_eq!(detection.rule_id, "prompt_idle");
}

#[test]
fn unsupported_region_is_rejected() {
    let err = Detector::from_toml(
        r#"
id = "bad"
[[rules]]
id = "r"
state = "idle"
priority = 1
region = "prompt_box_body"
contains = ["x"]
"#,
    )
    .unwrap_err();
    assert!(format!("{err:#}").contains("unsupported region"));
}
