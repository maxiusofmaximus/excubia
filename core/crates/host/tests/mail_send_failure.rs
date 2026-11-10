//! Why a send failed, as the host keeps it (MAIL-FAILCODE): each way Graph refuses
//! or loses a send ends in its code and retry rule, and a send that left has none.
//! Over the routed fake, so no test reaches Microsoft.

mod mail_send_support;

use std::time::Duration;

use excubia_core_chat::Decision;
use excubia_core_mail::{FailCode, Retry, SendFailure, SendState};
use mail_send_support::{approved_and_sent, ask, scheduled, settle, Host};
use serde_json::json;

/// Approves one send while Graph is set up by `arrange`, waits for the window to
/// end and returns the failure the ledger keeps for it.
async fn failure_after(name: &str, arrange: impl FnOnce(&Host)) -> SendFailure {
    let host = Host::start(name).await;
    let service = host.service(Duration::from_millis(50), 10);
    arrange(&host);
    let approval = ask(&service).await;
    host.decide(&approval, Decision::Approve);
    let send_id = scheduled(&service, &approval).await;
    assert_eq!(settle(&service, &send_id).await, "failed");
    service
        .status(&send_id)
        .expect("status")
        .failure
        .expect("a failed send says why")
}

#[tokio::test]
async fn an_account_microsoft_suspended_fails_as_account_suspended_and_is_not_retried() {
    let failure = failure_after("failcode-suspended", |host| {
        host.world.refuse_send(
            403,
            json!({"error": {"code": "ErrorAccountSuspend", "message": "ana@ejemplo.com token-secreto"}}),
        );
    })
    .await;

    assert_eq!(
        failure,
        SendFailure {
            code: FailCode::AccountSuspended,
            retry: Retry::No,
        }
    );
}

#[tokio::test]
async fn a_5xx_from_graph_fails_as_graph_unavailable_and_is_safe_to_retry() {
    let failure = failure_after("failcode-503", |host| {
        host.world
            .refuse_send(503, json!({"error": {"code": "serviceNotAvailable"}}));
    })
    .await;

    assert_eq!(
        failure,
        SendFailure {
            code: FailCode::GraphUnavailable,
            retry: Retry::Safe,
        }
    );
}

#[tokio::test]
async fn a_throttled_send_fails_as_throttled_and_is_safe_to_retry() {
    let failure = failure_after("failcode-429", |host| {
        host.world
            .refuse_send(429, json!({"error": {"code": "ApplicationThrottled"}}));
    })
    .await;

    assert_eq!(
        failure,
        SendFailure {
            code: FailCode::Throttled,
            retry: Retry::Safe,
        }
    );
}

#[tokio::test]
async fn a_post_that_times_out_may_have_been_sent_so_it_must_be_checked() {
    let failure = failure_after("failcode-timeout", |host| host.world.time_out_send()).await;

    assert_eq!(
        failure,
        SendFailure {
            code: FailCode::GraphUnavailable,
            retry: Retry::CheckSent,
        }
    );
}

#[tokio::test]
async fn a_draft_changed_inside_the_window_fails_as_draft_changed_and_is_safe_to_retry() {
    let host = Host::start("failcode-changed").await;
    let service = host.service(Duration::from_millis(300), 10);
    let approval = ask(&service).await;
    host.decide(&approval, Decision::Approve);
    let send_id = scheduled(&service, &approval).await;
    host.world.set_body("Texto cambiado durante la ventana.");

    assert_eq!(settle(&service, &send_id).await, "failed");

    assert_eq!(
        service.status(&send_id).expect("status").failure,
        Some(SendFailure {
            code: FailCode::DraftChanged,
            retry: Retry::Safe,
        })
    );
    assert!(host.world.sends().is_empty());
}

#[tokio::test]
async fn a_restart_during_the_post_fails_the_send_as_interrupted() {
    let host = Host::start("failcode-interrupted").await;
    let before = host.service(Duration::from_secs(5), 10);
    let approval = ask(&before).await;
    host.decide(&approval, Decision::Approve);
    let send_id = scheduled(&before, &approval).await;
    assert!(before
        .ledger()
        .move_send(&send_id, SendState::PendingSend, SendState::Sending)
        .expect("claim"));

    let restarted = host.service(Duration::from_secs(5), 10);
    restarted.recover().expect("recover");

    let status = restarted.status(&send_id).expect("status");
    assert_eq!(status.state.public(), "failed");
    assert_eq!(
        status.failure,
        Some(SendFailure {
            code: FailCode::Interrupted,
            retry: Retry::CheckSent,
        })
    );
    assert!(host.world.sends().is_empty());
    host.expect_audit("\"result\":\"failed\"").await;
}

#[tokio::test]
async fn a_send_that_leaves_has_no_failure() {
    let host = Host::start("failcode-sent").await;
    let service = host.service(Duration::from_millis(50), 10);

    let send_id = approved_and_sent(&host, &service).await;

    let status = service.status(&send_id).expect("status");
    assert_eq!(status.state.public(), "sent");
    assert_eq!(status.failure, None);
}
