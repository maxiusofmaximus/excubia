//! The failure of a send over HTTP (MAIL-FAILCODE): `GET /v1/mail/sends/{id}` says
//! `failure: {code, retry}` for a failed send, and nothing for one that left. The
//! send window is ten seconds, so the rows are written to the ledger the way the
//! host writes them, and the route is asked over HTTP.

mod harness;

use std::time::{SystemTime, UNIX_EPOCH};

use harness::mail_send_support::{ACCOUNT, DRAFT};
use harness::TestHost;
use hn_core_auth::Permission;
use hn_core_host::mail_tools::LEDGER_FILE;
use hn_core_mail::{FailCode, Ledger, Retry, SendFailure, SendRow, SendState};
use serde_json::Value;

/// Writes one send the way the host does: created in `Sending`, then either sent or
/// failed with `failure`.
fn write_send(host: &TestHost, send_id: &str, failure: Option<SendFailure>) {
    let ledger = Ledger::open(&host.data_dir.join(LEDGER_FILE)).expect("ledger");
    let created_at = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs()),
    )
    .unwrap_or(i64::MAX);
    ledger
        .insert_send(&SendRow {
            send_id: send_id.to_owned(),
            draft_id: DRAFT.to_owned(),
            account: ACCOUNT.to_owned(),
            state: SendState::Sending,
            digest: format!("sha256:{send_id}"),
            approval_id: format!("approval-{send_id}"),
            domains: vec!["example.org".to_owned()],
            created_at,
            failure: None,
        })
        .expect("insert");
    match failure {
        Some(failure) => ledger
            .fail_send(send_id, SendState::Sending, failure)
            .expect("fail"),
        None => ledger
            .move_send(send_id, SendState::Sending, SendState::Sent)
            .expect("sent"),
    };
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_send_says_its_code_and_retry_and_nothing_else() {
    let (host, _world) = TestHost::start_with_mail("mail-http-failcode").await;
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;
    write_send(
        &host,
        "send-suspended",
        Some(SendFailure {
            code: FailCode::AccountSuspended,
            retry: Retry::No,
        }),
    );

    let (status, text) = approver
        .request("GET", "/v1/mail/sends/send-suspended", None)
        .await
        .expect("request");

    assert_eq!(status, 200, "{text}");
    let answer: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(answer["state"], "failed");
    assert_eq!(answer["failure"]["code"], "account_suspended");
    assert_eq!(answer["failure"]["retry"], "no");
    assert_eq!(
        answer["failure"].as_object().map(serde_json::Map::len),
        Some(2),
        "the failure serves its code and retry only: {text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn each_failure_code_is_served_with_its_retry_rule() {
    let (host, _world) = TestHost::start_with_mail("mail-http-failcode-table").await;
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;
    let rows = [
        (
            FailCode::GraphUnavailable,
            Retry::Safe,
            "graph_unavailable",
            "safe",
        ),
        (FailCode::Throttled, Retry::Safe, "throttled", "safe"),
        (FailCode::DraftChanged, Retry::Safe, "draft_changed", "safe"),
        (
            FailCode::Interrupted,
            Retry::CheckSent,
            "interrupted",
            "check_sent",
        ),
        (FailCode::Unknown, Retry::CheckSent, "unknown", "check_sent"),
    ];
    for (index, (code, retry, code_word, retry_word)) in rows.into_iter().enumerate() {
        let send_id = format!("send-row-{index}");
        write_send(&host, &send_id, Some(SendFailure { code, retry }));

        let (status, text) = approver
            .request("GET", &format!("/v1/mail/sends/{send_id}"), None)
            .await
            .expect("request");

        assert_eq!(status, 200, "{text}");
        let answer: Value = serde_json::from_str(&text).expect("json");
        assert_eq!(answer["failure"]["code"], code_word, "{text}");
        assert_eq!(answer["failure"]["retry"], retry_word, "{text}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_send_that_left_serves_no_failure() {
    let (host, _world) = TestHost::start_with_mail("mail-http-failcode-sent").await;
    let approver = host
        .paired_with("aprobador", &[Permission::Approve, Permission::ReadState])
        .await;
    write_send(&host, "send-left", None);

    let (status, text) = approver
        .request("GET", "/v1/mail/sends/send-left", None)
        .await
        .expect("request");

    assert_eq!(status, 200, "{text}");
    let answer: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(answer["state"], "sent");
    assert!(answer.get("failure").is_none(), "{text}");
}
