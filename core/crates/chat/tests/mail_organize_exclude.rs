//! Leaving messages out of a batch of mail moves (F2C1): the operator approves the
//! batch without some of its messages. The excluded ones stay in the payload, marked;
//! the digest does not move; and a refused exclusion decides nothing.

mod common;

use common::{team, Team};
use hn_core_chat::{
    action_digest, ApprovalKind, ApprovalStatus, ChatError, Decision, NewApproval, Reader,
    DEFAULT_TTL_SECONDS,
};
use serde_json::{json, Value};

/// A question for `tool` with `args` as its payload, asked from the project root.
fn question(team: &Team, tool: &str, args: Value) -> NewApproval {
    let cwd = team.root.to_string_lossy().to_string();
    let paths: Vec<String> = Vec::new();
    NewApproval {
        conversation_id: Some(team.conversation.clone()),
        seat_id: None,
        agent_label: None,
        run_id: None,
        pane_id: None,
        kind: ApprovalKind::Tool,
        tool: tool.to_string(),
        summary: format!("Acción {tool}"),
        cwd: Some(cwd.clone()),
        paths: paths.clone(),
        risk: None,
        action_digest: action_digest("tool", tool, &args, &cwd, &paths),
        payload_redacted: Some(args),
        ttl_seconds: DEFAULT_TTL_SECONDS,
    }
}

/// A batch of mail moves whose payload lists `ids` the way the host proposes them.
fn listed_batch(team: &Team, ids: &[&str]) -> NewApproval {
    let items: Vec<Value> = ids
        .iter()
        .map(|id| {
            json!({
                "message_id": id,
                "dest": "junkemail",
                "reason": "Publicidad",
                "from_address": "promo@tienda.example",
                "subject": "Oferta",
                "level": "low",
            })
        })
        .collect();
    question(
        team,
        "mail.move",
        json!({ "account": "trabajo", "items": items }),
    )
}

fn excluding(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn an_approve_leaves_out_the_excluded_messages_and_keeps_the_list_whole() {
    let team = team("organize-exclude");
    let approval = team
        .store
        .create_mail_organize_approval(listed_batch(&team, &["m1", "m2", "m3"]))
        .expect("the batch is filed");

    let decided = team
        .store
        .decide_approval_excluding(
            &approval.id,
            Decision::Approve,
            &excluding(&["m2"]),
            "client-1",
            "device-1",
        )
        .expect("the operator approves without m2");

    assert_eq!(decided.status, ApprovalStatus::Approved);
    assert_eq!(decided.action_digest, approval.action_digest);
    let items = decided.payload_redacted.as_ref().expect("payload")["items"].clone();
    assert_eq!(items.as_array().map(Vec::len), Some(3), "{items}");
    assert_eq!(items[0].get("excluded"), None);
    assert_eq!(items[1]["excluded"], true);
    assert_eq!(items[2].get("excluded"), None);
    assert_eq!(
        decided.view(Reader::Operator)["details"]["items"][1]["excluded"],
        true
    );
    assert!(decided.view(Reader::Other).get("details").is_none());
}

#[test]
fn an_exclusion_must_name_a_message_of_the_batch_and_decides_nothing() {
    let team = team("organize-exclude-unknown");
    let approval = team
        .store
        .create_mail_organize_approval(listed_batch(&team, &["m1", "m2"]))
        .expect("the batch is filed");

    let refused = team
        .store
        .decide_approval_excluding(
            &approval.id,
            Decision::Approve,
            &excluding(&["ghost"]),
            "client-1",
            "device-1",
        )
        .expect_err("ghost is not in the batch");

    assert!(
        matches!(refused, ChatError::NoSuchMessage(ref id) if id == "ghost"),
        "{refused:?}"
    );
    let still = team.store.approval(&approval.id).expect("approval");
    assert_eq!(still.status, ApprovalStatus::Pending);
}

#[test]
fn excluding_every_message_is_refused_and_decides_nothing() {
    let team = team("organize-exclude-all");
    let approval = team
        .store
        .create_mail_organize_approval(listed_batch(&team, &["m1", "m2"]))
        .expect("the batch is filed");

    let refused = team
        .store
        .decide_approval_excluding(
            &approval.id,
            Decision::Approve,
            &excluding(&["m1", "m2"]),
            "client-1",
            "device-1",
        )
        .expect_err("nothing would move");

    assert!(
        matches!(refused, ChatError::InvalidExclusion(_)),
        "{refused:?}"
    );
    let still = team.store.approval(&approval.id).expect("approval");
    assert_eq!(still.status, ApprovalStatus::Pending);
}

#[test]
fn a_denial_takes_no_exclusions() {
    let team = team("organize-exclude-deny");
    let approval = team
        .store
        .create_mail_organize_approval(listed_batch(&team, &["m1", "m2"]))
        .expect("the batch is filed");

    let refused = team
        .store
        .decide_approval_excluding(
            &approval.id,
            Decision::Deny,
            &excluding(&["m1"]),
            "client-1",
            "device-1",
        )
        .expect_err("a denial has no exclusions");

    assert!(
        matches!(refused, ChatError::InvalidExclusion(_)),
        "{refused:?}"
    );
    let still = team.store.approval(&approval.id).expect("approval");
    assert_eq!(still.status, ApprovalStatus::Pending);
}

#[test]
fn only_a_batch_of_mail_moves_takes_exclusions() {
    let team = team("organize-exclude-tool");
    let send = team
        .store
        .create_mail_send_approval(question(
            &team,
            "mail.send",
            json!({ "cuenta": "trabajo", "borrador": "draft-1" }),
        ))
        .expect("the send is filed");

    let refused = team
        .store
        .decide_approval_excluding(
            &send.id,
            Decision::Approve,
            &excluding(&["draft-1"]),
            "client-1",
            "device-1",
        )
        .expect_err("a send takes no exclusions");

    assert!(
        matches!(refused, ChatError::InvalidExclusion(_)),
        "{refused:?}"
    );
}

#[test]
fn the_same_client_gets_the_same_answer_and_a_new_one_cannot_change_the_exclusions() {
    let team = team("organize-exclude-again");
    let approval = team
        .store
        .create_mail_organize_approval(listed_batch(&team, &["m1", "m2", "m3"]))
        .expect("the batch is filed");
    let first = team
        .store
        .decide_approval_excluding(
            &approval.id,
            Decision::Approve,
            &excluding(&["m2"]),
            "client-1",
            "device-1",
        )
        .expect("the operator approves");

    let again = team
        .store
        .decide_approval_excluding(
            &approval.id,
            Decision::Approve,
            &excluding(&["m2"]),
            "client-1",
            "device-1",
        )
        .expect("the same client gets the same answer");
    assert_eq!(again.payload_redacted, first.payload_redacted);

    let other = team
        .store
        .decide_approval_excluding(
            &approval.id,
            Decision::Approve,
            &excluding(&["m1"]),
            "client-2",
            "device-1",
        )
        .expect_err("a second client finds it decided");
    assert!(
        matches!(other, ChatError::ApprovalAlreadyDecided),
        "{other:?}"
    );
    let decided = team.store.approval(&approval.id).expect("approval");
    let items = &decided.payload_redacted.expect("payload")["items"];
    assert_eq!(items[0].get("excluded"), None, "m1 stays in the batch");
}

#[test]
fn the_timeline_line_of_a_decided_batch_says_how_many_were_left_out() {
    let team = team("organize-exclude-line");
    let approval = team
        .store
        .create_mail_organize_approval(listed_batch(&team, &["m1", "m2", "m3"]))
        .expect("the batch is filed");
    team.store
        .decide_approval_excluding(
            &approval.id,
            Decision::Approve,
            &excluding(&["m2", "m3"]),
            "client-1",
            "device-1",
        )
        .expect("the operator approves without m2 and m3");

    let entries = team
        .store
        .timeline(&team.conversation, None, 50)
        .expect("timeline")
        .entries;
    let line = entries
        .iter()
        .find(|entry| entry.ref_id.as_deref() == Some(approval.id.as_str()))
        .and_then(|entry| entry.summary.clone())
        .expect("the batch has its line");
    assert!(line.ends_with(" · dejaste fuera 2"), "{line}");
}
