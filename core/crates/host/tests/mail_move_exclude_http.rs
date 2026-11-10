//! The operator leaves messages out of a batch over HTTP (F2C1): the decision takes
//! `exclude`, bounded and only from a device with `APPROVE`. Every refusal has a stable
//! code and decides nothing.

mod harness;

use std::time::Duration;

use harness::{detail, SeatKind, TestHost};
use excubia_core_auth::Permission;
use excubia_core_chat::{action_digest, ApprovalKind, ApprovalStatus, NewApproval, DEFAULT_TTL_SECONDS};
use serde_json::{json, Value};

const TTL: Duration = Duration::from_secs(45);

/// A batch of mail moves to junk for `ids`, as the host proposes it.
fn batch(ids: &[&str]) -> NewApproval {
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
    let args = json!({ "account": "trabajo", "items": items });
    let paths: Vec<String> = Vec::new();
    NewApproval {
        conversation_id: None,
        seat_id: None,
        agent_label: None,
        run_id: None,
        pane_id: None,
        kind: ApprovalKind::Tool,
        tool: "mail.move".to_owned(),
        summary: "Mover correos".to_owned(),
        cwd: None,
        paths: paths.clone(),
        risk: None,
        action_digest: action_digest("tool", "mail.move", &args, "", &paths),
        payload_redacted: Some(args),
        ttl_seconds: DEFAULT_TTL_SECONDS,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_operator_approves_a_batch_without_some_of_its_messages() {
    let host = TestHost::start_with_seat("mail-exclude-http", TTL, SeatKind::Fake).await;
    let (client, paired) = host.pair("Desktop correo").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    host.grant(&paired.device_id, Permission::Approve);
    let approval = host
        .chat
        .create_mail_organize_approval(batch(&["m1", "m2", "m3"]))
        .expect("the batch is filed");

    let decided = client
        .chat_decide_approval_excluding(&approval.id, "approve", "client-1", &["m2"])
        .await
        .expect("the decision");

    assert_eq!(decided["approval"]["status"], "approved");
    let items = &decided["approval"]["details"]["items"];
    assert_eq!(items[1]["excluded"], true);
    assert_eq!(items[0].get("excluded"), None);
    assert_eq!(items.as_array().map(Vec::len), Some(3));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_exclusions_are_bounded_and_refused_with_stable_codes() {
    let host = TestHost::start_with_seat("mail-exclude-refused", TTL, SeatKind::Fake).await;
    let (client, paired) = host.pair("Desktop correo").await;
    host.grant(&paired.device_id, Permission::ReadState);
    host.grant(&paired.device_id, Permission::Respond);
    host.grant(&paired.device_id, Permission::Approve);
    let (outsider, _) = host.pair("Sin permisos").await;
    let approval = host
        .chat
        .create_mail_organize_approval(batch(&["m1", "m2"]))
        .expect("the batch is filed");
    let id = approval.id.as_str();

    let too_many: Vec<String> = (0..21).map(|n| format!("m{n}")).collect();
    let too_many: Vec<&str> = too_many.iter().map(String::as_str).collect();
    let err = client
        .chat_decide_approval_excluding(id, "approve", "client-1", &too_many)
        .await
        .expect_err("21 exclusions are over the bound");
    assert_eq!(err.status, 422, "{}", err.body);
    assert_eq!(detail(&err.body)["field"], "exclude");

    let long = "x".repeat(513);
    let err = client
        .chat_decide_approval_excluding(id, "approve", "client-1", &[long.as_str()])
        .await
        .expect_err("an id over 512 characters is refused");
    assert_eq!(detail(&err.body)["field"], "exclude");

    let err = outsider
        .chat_decide_approval_excluding(id, "approve", "client-3", &["m1"])
        .await
        .expect_err("a device without APPROVE decides nothing");
    assert_eq!(err.status, 403, "{}", err.body);

    let err = client
        .chat_decide_approval_excluding(id, "approve", "client-1", &["ghost"])
        .await
        .expect_err("ghost is not in the batch");
    assert_eq!(detail(&err.body)["code"], "unknown_message");

    let err = client
        .chat_decide_approval_excluding(id, "approve", "client-1", &["m1", "m2"])
        .await
        .expect_err("excluding everything is refused");
    assert_eq!(detail(&err.body)["code"], "invalid_exclusion");

    let err = client
        .chat_decide_approval_excluding(id, "deny", "client-1", &["m1"])
        .await
        .expect_err("a denial takes no exclusions");
    assert_eq!(detail(&err.body)["code"], "invalid_exclusion");

    let still = host.chat.approval(id).expect("approval");
    assert_eq!(still.status, ApprovalStatus::Pending);
}
