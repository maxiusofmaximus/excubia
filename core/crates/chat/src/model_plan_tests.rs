use super::*;

fn step(title: &str, touches: &[Touch]) -> PlanStep {
    PlanStep {
        title: title.to_string(),
        touches: touches.to_vec(),
    }
}

fn steps(count: usize) -> Vec<PlanStep> {
    (0..count)
        .map(|index| step(&format!("Paso {index}"), &[Touch::Files]))
        .collect()
}

fn invalid(result: Result<(), ChatError>) -> bool {
    matches!(result, Err(ChatError::InvalidPlan(_)))
}

#[test]
fn a_plan_has_between_one_and_twenty_steps() {
    assert!(validate(&steps(1)).is_ok());
    assert!(validate(&steps(MAX_STEPS)).is_ok());
    assert!(invalid(validate(&steps(0))), "no steps");
    assert!(invalid(validate(&steps(MAX_STEPS + 1))), "21 steps");
}

#[test]
fn a_title_is_plain_text_of_one_to_120_characters() {
    assert!(validate(&[step(&"x".repeat(120), &[Touch::Tools])]).is_ok());
    assert!(
        invalid(validate(&[step(&"x".repeat(121), &[Touch::Tools])])),
        "121"
    );
    assert!(invalid(validate(&[step("   ", &[Touch::Tools])])), "blank");
    assert!(
        invalid(validate(&[step("Abre <script>", &[Touch::Tools])])),
        "html"
    );
    assert!(
        invalid(validate(&[step("Ejecuta `ls`", &[Touch::Tools])])),
        "backticks"
    );
    assert!(
        invalid(validate(&[step("Linea\nDos", &[Touch::Tools])])),
        "newline"
    );
}

#[test]
fn a_step_says_what_it_touches() {
    assert!(invalid(validate(&[step("Sin tocar", &[])])), "no touches");
    assert!(validate(&[step("Nada", &[Touch::Nothing])]).is_ok());
}

#[test]
fn the_summary_counts_steps_names_the_proposer_and_lists_touches_in_order() {
    let plan = vec![
        step("Uno", &[Touch::Network, Touch::Files]),
        step("Dos", &[Touch::Files]),
    ];
    assert_eq!(
        summary(&plan, "Ernesto"),
        "Plan de 2 pasos · lo propone Ernesto · toca: ficheros, internet"
    );
    assert_eq!(
        summary(&steps(1), "Ernesto"),
        "Plan de 1 paso · lo propone Ernesto · toca: ficheros"
    );
}

#[test]
fn only_mail_and_network_reach_out() {
    assert!(reaches_out(&[Touch::Files, Touch::Mail]));
    assert!(reaches_out(&[Touch::Network]));
    assert!(!reaches_out(&[Touch::Files, Touch::Tools, Touch::Nothing]));
}

#[test]
fn a_note_is_plain_text_of_at_most_300_characters() {
    assert!(validate_note("Quita el paso 3").is_ok());
    assert!(validate_note(&"n".repeat(300)).is_ok());
    assert!(invalid(validate_note(&"n".repeat(301))), "301");
    assert!(invalid(validate_note("<b>no</b>")), "markup");
}
