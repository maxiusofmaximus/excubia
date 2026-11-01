//! Session tests.

use std::time::{Duration, Instant};

use hn_core_detect::DetectedState;

use super::detect::QueryResponder;
use super::handle::Session;

#[test]
fn the_query_responder_sees_split_sequences() {
    let mut responder = QueryResponder::default();
    assert!(!responder.scan(b"\x1b["));
    assert!(responder.scan(b"6n"));
    assert!(!responder.scan(b"hola"));
}

#[test]
fn a_restored_session_serves_its_text_and_refuses_writes() {
    let session = Session::restored("uno\ndos\ntres".to_string());
    assert_eq!(session.read(2), "dos\ntres");
    assert!(session.write(b"y").is_err());
    assert!(!session.is_live());
}

#[test]
fn a_hook_report_is_fresh_until_its_ttl_passes() {
    let session = Session::restored(String::new());
    let now = Instant::now();
    session.set_hook(DetectedState::Blocked, now);
    assert_eq!(
        session.fresh_hook(Duration::from_secs(5), now),
        Some(DetectedState::Blocked)
    );
    assert_eq!(
        session.fresh_hook(Duration::from_secs(5), now + Duration::from_secs(6)),
        None
    );
    assert!(session
        .fresh_hook_remaining(Duration::from_secs(5), now + Duration::from_secs(6))
        .is_none());
}
