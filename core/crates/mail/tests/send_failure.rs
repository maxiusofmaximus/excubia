//! Why a send failed (MAIL-FAILCODE): one test per row of the cause table, the
//! words the API may say, and a ledger that keeps the code next to the state and
//! reads the rows of an earlier build as the conservative legacy failure.

use std::time::{SystemTime, UNIX_EPOCH};

use excubia_core_mail::{
    FailCode, Ledger, MailError, Redacted, Retry, SendFailure, SendResult, SendRow, SendState,
};
use excubia_core_test_dir::TestDir;
use rusqlite::Connection;

fn graph(status: u16, code: &str) -> MailError {
    MailError::Graph {
        status,
        code: code.to_owned(),
        message: Redacted::new("ana@ejemplo.com token-secreto"),
    }
}

fn of(error: MailError, posted: bool) -> (FailCode, Retry) {
    let failure = SendFailure::of(&error, posted);
    (failure.code, failure.retry)
}

#[test]
fn an_account_suspended_by_microsoft_is_final() {
    let suspended = graph(403, "ErrorAccountSuspend");
    assert_eq!(
        of(suspended, false),
        (FailCode::AccountSuspended, Retry::No)
    );
    assert_eq!(
        of(graph(403, "ErrorAccountSuspend"), true),
        (FailCode::AccountSuspended, Retry::No)
    );
}

#[test]
fn a_rejected_sign_in_needs_the_owner_and_a_plain_401_is_retried_after_login() {
    assert_eq!(
        of(MailError::AuthRequired, false),
        (FailCode::AuthExpired, Retry::Safe)
    );
    let rejected = MailError::AuthRejected {
        code: "invalid_grant".to_owned(),
        message: Redacted::new("revoked"),
    };
    assert_eq!(of(rejected, false), (FailCode::AuthExpired, Retry::No));
    let consent = MailError::AdminConsentRequired {
        detail: Redacted::new("AADSTS65001"),
    };
    assert_eq!(of(consent, false), (FailCode::AuthExpired, Retry::No));
    let unregistered = MailError::AppNotRegistered {
        detail: Redacted::new("AADSTS700016"),
    };
    assert_eq!(of(unregistered, false), (FailCode::AuthExpired, Retry::No));
}

#[test]
fn a_throttled_post_is_refused_and_safe_to_retry() {
    assert_eq!(
        of(graph(429, "ApplicationThrottled"), true),
        (FailCode::Throttled, Retry::Safe)
    );
}

#[test]
fn a_5xx_before_graph_accepts_is_unavailable_and_safe_to_retry() {
    assert_eq!(
        of(graph(503, "serviceNotAvailable"), true),
        (FailCode::GraphUnavailable, Retry::Safe)
    );
    assert_eq!(
        of(graph(500, "InternalServerError"), false),
        (FailCode::GraphUnavailable, Retry::Safe)
    );
}

#[test]
fn a_4xx_that_is_not_a_known_refusal_is_a_bad_request_and_safe_to_retry() {
    assert_eq!(
        of(graph(400, "ErrorInvalidRequest"), true),
        (FailCode::BadRequest, Retry::Safe)
    );
    assert_eq!(
        of(graph(403, "ErrorAccessDenied"), true),
        (FailCode::BadRequest, Retry::Safe)
    );
}

#[test]
fn a_network_error_before_the_post_is_unavailable_and_safe_to_retry() {
    let network = || MailError::Transport(Redacted::new("connection refused"));
    assert_eq!(
        of(network(), false),
        (FailCode::GraphUnavailable, Retry::Safe)
    );
}

#[test]
fn a_timeout_on_the_post_may_have_sent_the_message() {
    let timeout = || MailError::Transport(Redacted::new("the request timed out"));
    assert_eq!(
        of(timeout(), true),
        (FailCode::GraphUnavailable, Retry::CheckSent)
    );
}

#[test]
fn an_unreadable_answer_is_unknown_and_may_have_sent_the_message() {
    let answer = MailError::BadResponse(Redacted::new("the HTTP answer is not UTF-8"));
    assert_eq!(of(answer, true), (FailCode::Unknown, Retry::CheckSent));
}

#[test]
fn a_draft_changed_after_approval_is_stale_and_nothing_was_sent() {
    assert_eq!(
        (
            SendFailure::DRAFT_CHANGED.code,
            SendFailure::DRAFT_CHANGED.retry
        ),
        (FailCode::DraftChanged, Retry::Safe)
    );
    assert_eq!(SendFailure::DRAFT_CHANGED.audit_result(), SendResult::Stale);
}

#[test]
fn a_send_the_host_lost_mid_post_may_have_left() {
    assert_eq!(
        (
            SendFailure::INTERRUPTED.code,
            SendFailure::INTERRUPTED.retry
        ),
        (FailCode::Interrupted, Retry::CheckSent)
    );
    assert_eq!(SendFailure::INTERRUPTED.audit_result(), SendResult::Failed);
}

#[test]
fn a_failure_without_a_code_reads_as_unknown_and_may_have_left() {
    assert_eq!(
        (SendFailure::LEGACY.code, SendFailure::LEGACY.retry),
        (FailCode::Unknown, Retry::CheckSent)
    );
}

#[test]
fn every_code_and_retry_has_a_stable_word_and_parses_back() {
    let codes = [
        FailCode::AccountSuspended,
        FailCode::AuthExpired,
        FailCode::Throttled,
        FailCode::GraphUnavailable,
        FailCode::BadRequest,
        FailCode::DraftChanged,
        FailCode::Interrupted,
        FailCode::Unknown,
    ];
    for code in codes {
        assert_eq!(FailCode::parse(code.as_str()), Some(code));
    }
    for retry in [Retry::Safe, Retry::CheckSent, Retry::No] {
        assert_eq!(Retry::parse(retry.as_str()), Some(retry));
    }
    assert_eq!(FailCode::parse("tokens o texto libre"), None);
    assert_eq!(Retry::parse("maybe"), None);
}

fn row(send_id: &str, approval_id: &str) -> SendRow {
    SendRow {
        send_id: send_id.to_owned(),
        draft_id: "draft-1".to_owned(),
        account: "trabajo".to_owned(),
        state: SendState::PendingSend,
        digest: "sha256:digest".to_owned(),
        approval_id: approval_id.to_owned(),
        domains: vec!["ejemplo.com".to_owned()],
        created_at: now(),
        failure: None,
    }
}

fn now() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs()),
    )
    .unwrap_or(i64::MAX)
}

#[test]
fn a_failure_is_stored_with_its_state_and_read_back_as_it_was() {
    let dir = TestDir::new("mail-failcode-store");
    let ledger = Ledger::open(&dir.join("ledger.db")).expect("ledger");
    ledger
        .insert_send(&row("send-1", "approval-1"))
        .expect("insert");

    assert!(ledger
        .move_send("send-1", SendState::PendingSend, SendState::Sending)
        .expect("move"));
    let failure = SendFailure {
        code: FailCode::AccountSuspended,
        retry: Retry::No,
    };
    assert!(ledger
        .fail_send("send-1", SendState::Sending, failure)
        .expect("fail"));

    let stored = ledger.send_row("send-1").expect("read").expect("row");
    assert_eq!(stored.state, SendState::Failed);
    assert_eq!(stored.failure, Some(failure));
}

#[test]
fn a_sent_send_has_no_failure() {
    let dir = TestDir::new("mail-failcode-sent");
    let ledger = Ledger::open(&dir.join("ledger.db")).expect("ledger");
    ledger
        .insert_send(&row("send-2", "approval-2"))
        .expect("insert");
    assert!(ledger
        .move_send("send-2", SendState::PendingSend, SendState::Sending)
        .expect("move"));
    assert!(ledger
        .move_send("send-2", SendState::Sending, SendState::Sent)
        .expect("move"));

    let stored = ledger.send_row("send-2").expect("read").expect("row");
    assert_eq!(stored.state, SendState::Sent);
    assert_eq!(stored.failure, None);
}

#[test]
fn a_ledger_from_before_codes_keeps_its_rows_and_reads_them_as_unknown() {
    let dir = TestDir::new("mail-failcode-legacy");
    let path = dir.join("ledger.db");
    {
        let connection = Connection::open(&path).expect("old ledger");
        connection
            .execute_batch(
                "CREATE TABLE mail_send (
                    send_id TEXT PRIMARY KEY, draft_id TEXT NOT NULL, account TEXT NOT NULL,
                    state TEXT NOT NULL, digest TEXT NOT NULL, approval_id TEXT NOT NULL UNIQUE,
                    domains TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
                );",
            )
            .expect("old schema");
        connection
            .execute(
                "INSERT INTO mail_send VALUES
                    ('old-send', 'draft-1', 'trabajo', 'failed', 'sha256:digest',
                     'old-approval', 'ejemplo.com', ?1, ?1)",
                [now()],
            )
            .expect("old row");
    }

    let ledger = Ledger::open(&path).expect("migrated ledger");
    let old = ledger
        .send_row("old-send")
        .expect("read")
        .expect("old row kept");
    assert_eq!(old.state, SendState::Failed);
    assert_eq!(old.failure, Some(SendFailure::LEGACY));

    ledger
        .insert_send(&row("new-send", "new-approval"))
        .expect("insert after migration");
    assert_eq!(
        ledger
            .send_row("new-send")
            .expect("read")
            .expect("row")
            .failure,
        None
    );
}
