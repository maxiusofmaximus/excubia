//! `mail.draft_get` reads the body of a draft this host created (TASK-DRAFTBODY):
//! the text, cut at 12 000 characters and marked as untrusted content; a draft
//! this host did not create is refused before any body is asked. Over the
//! routed fake in `mail_send_support`. Zero network, zero real accounts.

mod mail_send_support;

use std::time::Duration;

use excubia_chat::SeatActor;
use excubia_seat::{DraftRef, MailCall};
use mail_send_support::{Host, ACCOUNT, BODY, DRAFT};

fn actor() -> SeatActor {
    SeatActor {
        agent_id: "alfred".into(),
        role_id: "alfred".into(),
        project_id: None,
        conversation_id: String::new(),
        run_id: "run-1".into(),
    }
}

fn draft_get(draft_id: &str) -> MailCall {
    MailCall::DraftGet(DraftRef {
        account: ACCOUNT.to_owned(),
        draft_id: draft_id.to_owned(),
    })
}

#[tokio::test]
async fn draft_get_returns_the_body_as_text_marked_untrusted() {
    let host = Host::start("mail-draft-body").await;
    let tools = host.tools(host.service(Duration::from_millis(50), 10));

    let reply = tools
        .call(&actor(), draft_get(DRAFT))
        .await
        .expect("draft_get");

    assert_eq!(reply.result["untrusted_content"], true);
    // A text body leaves the create path with a trailing no-break space, as Graph stores it.
    assert_eq!(reply.result["draft"]["body"], format!("{BODY}\u{a0}"));
    assert_eq!(reply.result["draft"]["body_truncated"], false);
}

#[tokio::test]
async fn a_body_over_12000_characters_is_cut_and_says_so() {
    let host = Host::start("mail-draft-body-long").await;
    let tools = host.tools(host.service(Duration::from_millis(50), 10));
    host.world.set_body(&"a".repeat(12_005));

    let reply = tools
        .call(&actor(), draft_get(DRAFT))
        .await
        .expect("draft_get");

    let body = reply.result["draft"]["body"].as_str().expect("body");
    assert_eq!(body.chars().count(), 12_000);
    assert_eq!(reply.result["draft"]["body_truncated"], true);
    assert_eq!(reply.result["untrusted_content"], true);
}

#[tokio::test]
async fn a_draft_this_host_did_not_make_is_refused_without_a_body_request() {
    let host = Host::start("mail-draft-body-foreign").await;
    let tools = host.tools(host.service(Duration::from_millis(50), 10));

    let refused = tools
        .call(&actor(), draft_get("AAMk-ajeno"))
        .await
        .expect_err("refused");

    // The fake answers only the host's own draft; any other GET would fail with another code.
    assert_eq!(refused.code, "mail_unknown_draft");
}
