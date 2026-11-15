//! An action the host files as outside the workspace (MAIL1D §0, NV-2) is for
//! the operator alone. It is placed the same way whether the seat that asks has
//! a project or not, so no lead, CTO or worker can let it through.

mod common;

use common::{assert_for_the_operator_alone, assert_no_agent_decides, team, Team};
use excubia_chat::{
    action_digest, ApprovalKind, ApprovalStatus, Decision, HookScope, NewApproval,
    DEFAULT_TTL_SECONDS,
};
use serde_json::json;

const OUTSIDE: &str = "C:/fuera/informe.xlsx";

/// The question the mail gate files: a tool named `mail.attach_path`, with the
/// workspace as its cwd and the file as its only path.
fn file_question(team: &Team, seat: Option<&str>) -> NewApproval {
    let args = json!({ "ruta": OUTSIDE });
    let cwd = team.root.to_string_lossy().to_string();
    let paths = [OUTSIDE.to_string()];
    NewApproval {
        conversation_id: Some(team.conversation.clone()),
        seat_id: seat.map(str::to_string),
        agent_label: None,
        run_id: None,
        pane_id: None,
        kind: ApprovalKind::Tool,
        tool: "mail.attach_path".to_string(),
        summary: "Adjuntar informe".to_string(),
        cwd: Some(cwd.clone()),
        paths: paths.to_vec(),
        risk: None,
        action_digest: action_digest("tool", "mail.attach_path", &args, &cwd, &paths),
        payload_redacted: Some(args),
        ttl_seconds: DEFAULT_TTL_SECONDS,
    }
}

/// The question the send gate files (MAIL2A): one draft, sent from one account.
fn send_question(team: &Team) -> NewApproval {
    let args = json!({ "cuenta": "trabajo", "borrador": "draft-1" });
    let cwd = team.root.to_string_lossy().to_string();
    let paths: [String; 0] = [];
    NewApproval {
        conversation_id: Some(team.conversation.clone()),
        seat_id: None,
        agent_label: None,
        run_id: None,
        pane_id: None,
        kind: ApprovalKind::Tool,
        tool: "mail.send".to_string(),
        summary: "Enviar el borrador".to_string(),
        cwd: Some(cwd.clone()),
        paths: paths.to_vec(),
        risk: None,
        action_digest: action_digest("tool", "mail.send", &args, &cwd, &paths),
        payload_redacted: Some(args),
        ttl_seconds: DEFAULT_TTL_SECONDS,
    }
}

/// A send is the user's alone as well: it is filed in the `mail_send` category,
/// so no seat, lead or CTO can decide it.
#[test]
fn a_send_question_is_the_operators_alone() {
    let team = team("send-question");
    let approval = team
        .store
        .create_mail_send_approval(send_question(&team))
        .expect("the question is filed");

    assert_for_the_operator_alone(&approval);
    assert_no_agent_decides(&team, &approval);
}

#[test]
fn with_a_project_no_agent_lets_the_file_out_and_the_device_does() {
    let team = team("outside-with-project");
    let question = file_question(&team, Some(&team.specialist));
    let approval = team
        .store
        .create_outside_approval(question)
        .expect("the question is filed");

    assert_for_the_operator_alone(&approval);
    assert_no_agent_decides(&team, &approval);

    let decided = team
        .store
        .decide_approval(&approval.id, Decision::Approve, "device-1", "device-1")
        .expect("the device decides");
    assert_eq!(decided.status, ApprovalStatus::Approved);
}

#[test]
fn without_a_project_no_agent_lets_the_file_out_and_the_device_does() {
    let team = team("outside-without-project");
    let question = file_question(&team, None);
    let approval = team
        .store
        .create_outside_approval(question)
        .expect("the question is filed");

    assert_for_the_operator_alone(&approval);
    assert_no_agent_decides(&team, &approval);

    let decided = team
        .store
        .decide_approval(&approval.id, Decision::Approve, "device-1", "device-1")
        .expect("the device decides");
    assert_eq!(decided.status, ApprovalStatus::Approved);
}

/// Each attach is its own question: a card that expired unanswered, or one already
/// spent on another draft, lets the same file be asked again. A denied one does not.
#[test]
fn the_same_file_is_asked_again_after_an_expired_or_spent_card_but_not_after_a_denial() {
    let team = team("outside-reask");
    let mut short = file_question(&team, Some(&team.specialist));
    short.ttl_seconds = 1;
    let expired = team
        .store
        .create_outside_approval(short)
        .expect("first card");
    std::thread::sleep(std::time::Duration::from_millis(1_200));

    let again = team
        .store
        .create_outside_approval(file_question(&team, Some(&team.specialist)))
        .expect("asked again after the card expired");
    assert_ne!(again.id, expired.id);
    team.store
        .decide_approval(&again.id, Decision::Approve, "device-1", "device-1")
        .expect("the device approves");
    team.store
        .consume_approval(&again.id, None, &HookScope::seat(&team.specialist))
        .expect("spent on one draft");

    let third = team
        .store
        .create_outside_approval(file_question(&team, Some(&team.specialist)))
        .expect("asked again for another draft");
    assert_eq!(third.status, ApprovalStatus::Pending);
    team.store
        .decide_approval(&third.id, Decision::Deny, "device-1", "device-1")
        .expect("the device denies");
    let refused = team
        .store
        .create_outside_approval(file_question(&team, Some(&team.specialist)))
        .expect_err("a denied file is not asked again");
    assert!(
        matches!(refused, excubia_chat::ChatError::ApprovalDigestRefused),
        "{refused}"
    );
}
