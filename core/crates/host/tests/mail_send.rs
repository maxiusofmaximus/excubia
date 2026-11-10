//! The send path of a host (MAIL2A), one behaviour per test, over the routed
//! fake in `mail_send_support`. Zero network, zero real accounts.

mod mail_send_support;

use std::sync::Arc;
use std::time::Duration;

use excubia_core_api::DraftEdit;
use excubia_core_chat::{Decision, ManualClock};
use excubia_core_mail::MailError;
use mail_send_support::{
    approved_and_sent, ask, scheduled, settle, Host, ACCOUNT, BODY, DRAFT, LOGIN_ACCESS,
    LOGIN_REFRESH, SEND_ACCESS, SEND_REFRESH, TO,
};

#[tokio::test]
async fn an_approved_draft_is_sent_once_with_the_send_token() {
    let host = Host::start("mail-send-approve").await;
    let service = host.service(Duration::from_millis(50), 10);

    approved_and_sent(&host, &service).await;

    assert_eq!(host.world.sends(), vec![SEND_ACCESS.to_owned()]);
    assert!(
        host.world
            .reads()
            .iter()
            .all(|bearer| bearer == LOGIN_ACCESS),
        "a draft was read with the send token"
    );
    host.expect_audit("\"result\":\"sent\"").await;
}

#[tokio::test]
async fn a_cancel_inside_the_window_sends_nothing() {
    let host = Host::start("mail-send-cancel").await;
    let service = host.service(Duration::from_millis(300), 10);
    let approval = ask(&service).await;
    host.decide(&approval, Decision::Approve);
    let send_id = scheduled(&service, &approval).await;

    assert_eq!(service.cancel(&send_id).expect("cancel"), "cancelled");
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(
        service.status(&send_id).expect("status").state.public(),
        "cancelled"
    );
    assert!(host.world.sends().is_empty());
    host.expect_audit("\"result\":\"cancelled\"").await;
}

#[tokio::test]
async fn a_draft_changed_before_the_owner_confirms_is_stale() {
    let host = Host::start("mail-send-stale").await;
    let service = host.service(Duration::from_millis(50), 10);
    let approval = ask(&service).await;
    host.decide(&approval, Decision::Approve);
    host.world.set_body("Texto cambiado antes de confirmar.");

    let refused = service
        .send(ACCOUNT, DRAFT, Some(&approval))
        .await
        .expect_err("a changed draft must not be scheduled");

    assert_eq!(refused.code(), "mail_approval_stale");
    assert!(host.world.sends().is_empty());
}

#[tokio::test]
async fn a_draft_changed_inside_the_window_fails_and_sends_nothing() {
    let host = Host::start("mail-send-window").await;
    let service = host.service(Duration::from_millis(300), 10);
    let approval = ask(&service).await;
    host.decide(&approval, Decision::Approve);
    let send_id = scheduled(&service, &approval).await;

    host.world.set_body("Texto cambiado durante la ventana.");

    assert_eq!(settle(&service, &send_id).await, "failed");
    assert!(host.world.sends().is_empty());
}

#[tokio::test]
async fn one_approval_is_one_send() {
    let host = Host::start("mail-send-reuse").await;
    let service = host.service(Duration::from_millis(50), 10);
    let approval = ask(&service).await;
    host.decide(&approval, Decision::Approve);
    let send_id = scheduled(&service, &approval).await;
    assert_eq!(settle(&service, &send_id).await, "sent");

    let again = service
        .send(ACCOUNT, DRAFT, Some(&approval))
        .await
        .expect_err("a spent approval must not schedule a second send");

    assert_eq!(again.code(), "mail_approval_stale");
    assert_eq!(host.world.sends().len(), 1);
}

#[tokio::test]
async fn a_denied_approval_is_refused_and_audited() {
    let host = Host::start("mail-send-denied").await;
    let service = host.service(Duration::from_millis(50), 10);
    let approval = ask(&service).await;
    host.decide(&approval, Decision::Deny);

    let refused = service
        .send(ACCOUNT, DRAFT, Some(&approval))
        .await
        .expect_err("a denied approval must not schedule");

    assert_eq!(refused.code(), "mail_approval_denied");
    assert!(host.world.sends().is_empty());
    host.expect_audit("\"result\":\"denied\"").await;
}

#[tokio::test]
async fn a_host_without_the_send_grant_asks_nothing() {
    let host = Host::start("mail-send-disabled").await;
    let service = host.service(Duration::from_millis(50), 10);
    host.disable_send();

    let refused = service
        .send(ACCOUNT, DRAFT, None)
        .await
        .expect_err("sending is not enabled");

    assert_eq!(refused.code(), "mail_send_not_enabled");
    assert!(host.world.sends().is_empty());
}

#[tokio::test]
async fn the_hourly_limit_refuses_before_any_approval_is_asked() {
    let host = Host::start("mail-send-limit").await;
    let service = host.service(Duration::from_millis(50), 1);
    approved_and_sent(&host, &service).await;

    let refused = service
        .send(ACCOUNT, DRAFT, None)
        .await
        .expect_err("the second send in the hour is limited");

    assert_eq!(refused.code(), "mail_send_rate_limited");
    assert!(matches!(
        refused,
        MailError::SendRateLimited { retry_after } if retry_after > 0
    ));
    assert_eq!(host.world.sends().len(), 1);
}

#[tokio::test]
async fn a_restart_cancels_a_send_still_in_its_window() {
    let host = Host::start("mail-send-restart").await;
    let before = host.service(Duration::from_secs(5), 10);
    let approval = ask(&before).await;
    host.decide(&approval, Decision::Approve);
    let send_id = scheduled(&before, &approval).await;

    let restarted = host.service(Duration::from_secs(5), 10);
    restarted.recover().expect("recover");

    assert_eq!(
        restarted.status(&send_id).expect("status").state.public(),
        "cancelled"
    );
    assert!(host.world.sends().is_empty());
    host.expect_audit("\"result\":\"cancelled\"").await;
}

#[tokio::test]
async fn the_audit_keeps_domains_and_never_the_body_or_a_token() {
    let host = Host::start("mail-send-audit").await;
    let service = host.service(Duration::from_millis(50), 10);
    approved_and_sent(&host, &service).await;
    host.expect_audit("\"result\":\"sent\"").await;

    let audit = host.audit_text();

    assert!(
        audit.contains("example.org"),
        "the recipient domain is missing"
    );
    for secret in [
        TO,
        BODY,
        LOGIN_ACCESS,
        LOGIN_REFRESH,
        SEND_ACCESS,
        SEND_REFRESH,
    ] {
        assert!(!audit.contains(secret), "the audit leaked {secret}");
    }
}

/// The error of a call the host refused. A call that went through fails the test.
fn refusal<T>(result: Result<T, MailError>) -> MailError {
    match result {
        Ok(_) => panic!("the host did not refuse the call"),
        Err(error) => error,
    }
}

#[tokio::test]
async fn an_approval_that_expires_unanswered_is_audited_once_without_a_query() {
    let host = Host::start("mail-send-expire").await;
    let clock = Arc::new(ManualClock::at(1_000_000));
    host.chat.set_clock(clock.clone());
    let service = host.service(Duration::from_millis(50), 10);
    ask(&service).await;

    clock.advance_secs(86_400);
    service.watch();

    host.expect_audit("\"result\":\"expired\"").await;
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    let audit = host.audit_text();
    assert_eq!(
        audit.matches("\"result\":\"expired\"").count(),
        1,
        "{audit}"
    );
    assert!(!audit.contains("\"result\":\"denied\""), "{audit}");
}

#[tokio::test]
async fn a_denial_is_audited_once_however_often_the_send_is_asked_about() {
    let host = Host::start("mail-send-deny").await;
    let service = host.service(Duration::from_millis(50), 10);
    let approval = ask(&service).await;
    host.decide(&approval, Decision::Deny);
    service.watch();

    for _ in 0..3 {
        let error = refusal(service.send(ACCOUNT, DRAFT, Some(approval.as_str())).await);
        assert_eq!(error.code(), "mail_approval_denied");
    }
    host.expect_audit("\"result\":\"denied\"").await;
    tokio::time::sleep(Duration::from_millis(2_500)).await;

    let audit = host.audit_text();
    assert_eq!(audit.matches("\"result\":\"denied\"").count(), 1, "{audit}");
    assert!(host.world.sends().is_empty());
}

#[tokio::test]
async fn a_review_shows_the_body_and_the_recipients_and_sends_nothing() {
    let host = Host::start("mail-send-review").await;
    let service = host.service(Duration::from_millis(50), 10);

    let review = service.review(ACCOUNT, DRAFT).await.expect("review");

    assert_eq!(review.draft_id, DRAFT);
    assert_eq!(review.content_type, "text", "Graph holds the body as text");
    assert_eq!(review.text(), BODY, "the operator reads the text");
    assert_eq!(review.to, vec![TO.to_owned()]);
    assert!(host.world.sends().is_empty());
}

#[tokio::test]
async fn an_ascii_body_is_approved_without_the_hard_space_that_kept_it_utf8() {
    let host = Host::start("mail-send-ascii-padding").await;
    let service = host.service(Duration::from_millis(50), 10);

    let review = service.review(ACCOUNT, DRAFT).await.expect("review");
    assert_eq!(review.body, BODY);
    assert_eq!(review.text(), BODY);

    let approval = ask(&service).await;
    let details = host
        .chat
        .approval(&approval)
        .expect("the question")
        .payload_redacted
        .expect("the approval details");
    assert_eq!(details["body_preview"], BODY);

    host.decide(&approval, Decision::Approve);
    let send_id = scheduled(&service, &approval).await;
    assert_eq!(settle(&service, &send_id).await, "sent");
}

#[tokio::test]
async fn a_hard_space_the_owner_wrote_at_the_end_is_kept_and_still_sends() {
    let host = Host::start("mail-send-owner-nbsp").await;
    let owner_body = "Hola Álvarez\u{a0}";
    host.world.set_body(owner_body);
    let service = host.service(Duration::from_millis(50), 10);

    let review = service.review(ACCOUNT, DRAFT).await.expect("review");
    assert_eq!(review.text(), owner_body);

    let approval = ask(&service).await;
    host.decide(&approval, Decision::Approve);
    let send_id = scheduled(&service, &approval).await;
    assert_eq!(settle(&service, &send_id).await, "sent");
}

#[tokio::test]
async fn an_owner_edit_keeps_the_stored_body_or_stores_typed_text() {
    let host = Host::start("mail-send-edit-html").await;
    let service = host.service(Duration::from_millis(50), 10);
    let before = service.review(ACCOUNT, DRAFT).await.expect("review");

    let subject_only = DraftEdit {
        to: None,
        cc: None,
        subject: Some("Presupuesto de abril".to_owned()),
        body: None,
    };
    let kept = service
        .update_draft(ACCOUNT, DRAFT, subject_only, "dispositivo-1")
        .await
        .expect("subject edit");
    assert_eq!(kept.body, before.body, "the stored body goes back as it is");
    assert_eq!(kept.content_type, "text");
    assert_eq!(kept.text(), BODY);

    let typed = "Línea uno\n\nÁ ñ ü € <fin>";
    let body_edit = DraftEdit {
        to: None,
        cc: None,
        subject: None,
        body: Some(typed.to_owned()),
    };
    let edited = service
        .update_draft(ACCOUNT, DRAFT, body_edit, "dispositivo-1")
        .await
        .expect("body edit");
    assert_eq!(edited.body, typed, "typed text goes to Graph as it is");
    assert_eq!(edited.content_type, "text");
    assert_eq!(edited.text(), typed);
    let reread = service.review(ACCOUNT, DRAFT).await.expect("review");
    assert_eq!(reread.text(), typed, "the next read shows the same text");
}

#[tokio::test]
async fn a_draft_the_ledger_does_not_own_is_refused_before_microsoft_is_asked() {
    let host = Host::start("mail-send-foreign").await;
    let service = host.service(Duration::from_millis(50), 10);
    let reads = host.world.reads().len();

    let error = refusal(service.review(ACCOUNT, "AAMk-otro").await);
    assert_eq!(error.code(), "mail_unknown_draft");
    let error = refusal(service.send(ACCOUNT, "AAMk-otro", None).await);
    assert_eq!(error.code(), "mail_unknown_draft");

    assert_eq!(host.world.reads().len(), reads);
    assert!(host.world.sends().is_empty());
}
