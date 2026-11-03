//! PAKE pairing end to end (PAIRING-F2B3): a real host over a real TLS listener, and a test device that does what
//! the app will do (F2d): reads the exporter and the SPKI of the connection it is on, runs `hn-core-pake`, and
//! sends base64 messages. No phone is involved.

mod harness;
mod pake_support;

use hn_core_host::pairing_audit::AUDIT_FILE;
use hn_core_pake::Code;
use pake_support::*;
use serde_json::json;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn a_pake_pairing_leaves_a_pending_device_and_an_audit_without_secrets() {
    let rig = Rig::new("pake-ok").await;
    let device = device();
    let code = rig.arm().await;
    let mut conn = rig.connect().await;

    let started = start(&mut conn, &device, &code).await;
    assert_eq!(started.status, 200, "{}", started.answer);
    assert!(started.host_tag_ok(&conn.exporter, &conn.spki));
    let (status, paired) = finish(&mut conn, started.key.as_ref().expect("key")).await;

    assert_eq!(status, 200, "{paired}");
    assert_eq!(paired["ok"], true);
    assert_eq!(paired["permissions"], json!([]));
    assert!(paired["recovery_code"]
        .as_str()
        .is_some_and(|code| !code.is_empty()));
    let stored = rig
        .host
        .devices
        .get(paired["device_id"].as_str().expect("device id"))
        .expect("the device exists");
    assert!(
        stored.permissions.is_empty() && !stored.revoked,
        "pending, no permissions"
    );
    assert_eq!(stored.public_key, device.b64);
    assert_eq!(
        rig.events(),
        [
            ("pake_started".to_owned(), "pake".to_owned()),
            ("paired".to_owned(), "pake".to_owned())
        ]
    );
    let audit = std::fs::read_to_string(rig.host.data_dir.join(AUDIT_FILE)).expect("audit");
    for secret in [code.reveal(), &device.b64] {
        assert!(!audit.contains(secret), "a secret reached the audit file");
    }
    assert!(rig.ctx.pake_slot.take().is_none(), "the code was spent");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_code_gets_no_device_and_burns_the_code() {
    let rig = Rig::new("pake-wrong-code").await;
    let device = device();
    let right = rig.arm().await;
    let wrong = Code::generate().expect("rng");
    assert_ne!(wrong.reveal(), right.reveal());
    let mut conn = rig.connect().await;

    let started = start(&mut conn, &device, &wrong).await;
    assert_eq!(
        started.status, 200,
        "the host cannot tell a wrong code at start"
    );
    assert!(
        !started.host_tag_ok(&conn.exporter, &conn.spki),
        "the device rejects the host's tag"
    );
    let (status, refused) = finish(&mut conn, started.key.as_ref().expect("key")).await;
    assert_eq!((status, detail(&refused)), (403, "confirmation_failed"));
    assert!(rig.host.devices.is_empty(), "no device may be created");
    assert!(rig
        .events()
        .contains(&("pake_failed".to_owned(), "pake".to_owned())));

    let retry = start(&mut conn, &device, &right).await;
    assert_eq!((retry.status, detail(&retry.answer)), (410, "no_live_code"));
}

#[tokio::test(flavor = "multi_thread")]
async fn finish_may_use_another_connection_than_start() {
    let rig = Rig::new("pake-two-connections").await;
    let device = device();
    let code = rig.arm().await;
    let mut first = rig.connect().await;
    let started = start(&mut first, &device, &code).await;
    assert_eq!(started.status, 200, "{}", started.answer);

    let mut second = rig.connect().await;
    assert_ne!(first.exporter, second.exporter);
    let (status, paired) = finish(&mut second, started.key.as_ref().expect("key")).await;
    assert_eq!(status, 200, "{paired}");
    assert_eq!(rig.host.devices.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_relay_with_two_tls_sessions_cannot_make_the_tags_agree() {
    let rig = Rig::new("pake-relay").await;
    let device = device();
    let code = rig.arm().await;
    let mut first = rig.connect().await;
    let started = start(&mut first, &device, &code).await;
    let second = rig.connect().await;

    assert!(started.host_tag_ok(&first.exporter, &first.spki));
    assert!(
        !started.host_tag_ok(&second.exporter, &first.spki),
        "the host's tag must not verify under another connection's exporter"
    );
    assert!(
        !started.host_tag_ok(&first.exporter, &[7u8; 32]),
        "nor under another certificate"
    );

    // The other direction: the device's tag made for the first connection, replayed over the second.
    let mut second = second;
    let key = started.key.as_ref().expect("key");
    let (status, refused) = finish_for(&mut second, key, &first.exporter, &first.spki).await;
    assert_eq!((status, detail(&refused)), (403, "confirmation_failed"));
    assert!(rig.host.devices.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_start_with_the_same_code_is_gone_and_leaves_the_first_session_alone() {
    let rig = Rig::new("pake-second-start").await;
    let code = rig.arm().await;
    let mut conn = rig.connect().await;
    let legit = device();
    let started = start(&mut conn, &legit, &code).await;
    assert_eq!(started.status, 200, "{}", started.answer);

    let stray = start(&mut conn, &device(), &code).await;
    assert_eq!((stray.status, detail(&stray.answer)), (410, "no_live_code"));

    let (status, paired) = finish(&mut conn, started.key.as_ref().expect("key")).await;
    assert_eq!(
        status, 200,
        "a refused stray start must not end the real session: {paired}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_expired_code_and_an_expired_session_are_gone() {
    let rig = Rig::new("pake-expiry").await;
    let device = device();
    let mut conn = rig.connect().await;

    let code = rig.arm().await;
    rig.advance(Duration::from_secs(181));
    let late = start(&mut conn, &device, &code).await;
    assert_eq!((late.status, detail(&late.answer)), (410, "no_live_code"));

    let code = rig.arm().await;
    let started = start(&mut conn, &device, &code).await;
    assert_eq!(started.status, 200, "{}", started.answer);
    rig.advance(Duration::from_secs(61));
    let (status, gone) = finish(&mut conn, started.key.as_ref().expect("key")).await;
    assert_eq!((status, detail(&gone)), (410, "pake_session_gone"));
    assert!(rig.host.devices.is_empty());

    // Just inside the 60 s the session is still there.
    let code = rig.arm().await;
    let started = start(&mut conn, &device, &code).await;
    rig.advance(Duration::from_secs(59));
    let (status, paired) = finish(&mut conn, started.key.as_ref().expect("key")).await;
    assert_eq!(status, 200, "{paired}");
}
