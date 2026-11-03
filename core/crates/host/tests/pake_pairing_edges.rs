//! PAKE pairing end to end, second half (PAIRING-F2B3): malformed input, loopback, budgets, the admin route,
//! re-arming and the CLI helper. The rig is in `pake_support`.

mod harness;
mod pake_support;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use pake_support::*;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
async fn a_message_that_cannot_be_valid_is_a_400_and_only_the_ones_that_touch_the_code_burn_it() {
    let rig = Rig::new("pake-bad-messages").await;
    let device = device();
    let mut conn = rig.connect().await;

    // Shape errors are found before the code is taken: the code survives them.
    let code = rig.arm().await;
    let (_, good) = device_message(&rig.host_id, &device, &code);
    let off_curve = STANDARD.encode([&[4u8][..], &[1u8; 64]].concat());
    let shapes = [
        json!({ "device_pubkey": device.b64, "message": "!!not base64!!" }),
        json!({ "device_pubkey": device.b64, "message": STANDARD.encode(&good[..32]) }),
        json!({ "device_pubkey": device.b64, "message": STANDARD.encode([&good[..], &[0u8][..]].concat()) }),
        json!({ "device_pubkey": off_curve, "message": STANDARD.encode(&good) }),
        json!({ "device_pubkey": STANDARD.encode([4u8; 3]), "message": STANDARD.encode(&good) }),
        json!({ "message": STANDARD.encode(&good) }),
    ];
    for body in shapes {
        let (status, answer) = conn.post(START, &body).await;
        assert_eq!((status, detail(&answer)), (400, "invalid_body"), "{body}");
    }
    let alive = start(&mut conn, &device, &code).await;
    assert_eq!(
        alive.status, 200,
        "shape errors must not have burnt the code"
    );
    drop(alive);

    // Cryptographic errors are found with the code in hand: each one spends it.
    let identity_point = [&[b'A', 1u8][..], &[0u8; 31][..]].concat();
    for bad in [identity_point, [&b"B"[..], &good[1..]].concat()] {
        let code = rig.arm().await;
        let (status, answer) = conn.post(START, &start_body(&device, &bad)).await;
        assert_eq!((status, detail(&answer)), (400, "pake_invalid_message"));
        let next = start(&mut conn, &device, &code).await;
        assert_eq!(
            (next.status, detail(&next.answer)),
            (410, "no_live_code"),
            "the code was spent"
        );
    }
    assert!(rig
        .events()
        .contains(&("pake_failed".to_owned(), "pake".to_owned())));
    assert!(rig.host.devices.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn loopback_refuses_pake_and_leaves_the_code_alone() {
    let rig = Rig::new("pake-loopback").await;
    let device = device();
    let code = rig.arm().await;
    let (_, message) = device_message(&rig.host_id, &device, &code);

    let body = start_body(&device, &message).to_string();
    let (status, answer) = plain_post_body(rig.plain, START, None, &body).await;
    assert_eq!((status, detail(&answer)), (403, "pake_requires_tls"));
    let (status, answer) = plain_post_body(rig.plain, FINISH, None, r#"{"tag":"AAAA"}"#).await;
    assert_eq!((status, detail(&answer)), (403, "pake_requires_tls"));

    let mut conn = rig.connect().await;
    let started = start(&mut conn, &device, &code).await;
    assert_eq!(
        started.status, 200,
        "the refused loopback calls did not touch the code"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn pake_has_its_own_per_ip_budget() {
    let rig = Rig::new("pake-budget").await;
    let mut conn = rig.connect().await;
    // `WRITE_PER_MINUTE` of `hn_core_auth::ratelimit`, the size of the `pair:<ip>` bucket too.
    let limit = 30;

    for attempt in 0..limit {
        let (status, _) = conn.post(START, &json!({})).await;
        assert_eq!(status, 400, "attempt {attempt} is inside the budget");
    }
    let (status, answer) = conn.post(START, &json!({})).await;
    assert_eq!((status, detail(&answer)), (429, "slow down"));
    let (status, answer) = conn.post(FINISH, &json!({})).await;
    assert_eq!(
        (status, detail(&answer)),
        (429, "slow down"),
        "start and finish share the bucket"
    );
    // `/v1/pair` is a different bucket: it still answers (a 400 for the empty body, not a 429).
    let (status, _) = conn.post("/v1/pair", &json!({})).await;
    assert_eq!(status, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_host_token_on_loopback_arms_a_code() {
    let rig = Rig::new("pake-admin").await;

    let (status, _) = plain_post(rig.plain, ADMIN, None).await;
    assert_eq!(status, 401, "no token");
    let (status, _) = plain_post(rig.plain, ADMIN, Some("not-the-token")).await;
    assert_eq!(status, 401, "wrong token");
    let run = rig.host.run_hook("pake-admin");
    let (status, answer) = plain_post(rig.plain, ADMIN, Some(&run.token)).await;
    assert_eq!(
        (status, detail(&answer)),
        (403, "host_token_required"),
        "an agent's run token"
    );

    let (status, answer) = plain_post(rig.plain, ADMIN, Some(&rig.host.hook_token)).await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["code"].as_str().map(str::len), Some(8));
    assert_eq!(answer["expires_in"], 180);

    // The LAN listener does not have the route at all, token or not.
    let mut conn = rig.connect().await;
    let request = format!(
        "POST {ADMIN} HTTP/1.1\r\nHost: lan\r\nX-HN-Hook-Token: {}\r\nContent-Length: 0\r\n\r\n",
        rig.host.hook_token
    );
    let (status, _) = conn.raw(&request).await;
    assert_eq!(status, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn finish_needs_a_live_session_and_uses_it_once() {
    let rig = Rig::new("pake-finish-alone").await;
    let device = device();
    let mut conn = rig.connect().await;

    let (status, answer) = conn
        .post(FINISH, &json!({ "tag": STANDARD.encode([0u8; 32]) }))
        .await;
    assert_eq!(
        (status, detail(&answer)),
        (410, "pake_session_gone"),
        "no start before"
    );
    let (status, answer) = conn
        .post(FINISH, &json!({ "tag": STANDARD.encode([0u8; 31]) }))
        .await;
    assert_eq!(
        (status, detail(&answer)),
        (400, "invalid_body"),
        "a tag of the wrong size"
    );

    let code = rig.arm().await;
    let started = start(&mut conn, &device, &code).await;
    let key = started.key.as_ref().expect("key");
    let (status, _) = finish(&mut conn, key).await;
    assert_eq!(status, 200);
    let (status, answer) = finish(&mut conn, key).await;
    assert_eq!(
        (status, detail(&answer)),
        (410, "pake_session_gone"),
        "one use"
    );
    assert_eq!(rig.host.devices.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn arming_again_drops_the_session_of_the_earlier_code() {
    let rig = Rig::new("pake-rearm").await;
    let device = device();
    let mut conn = rig.connect().await;
    let first = rig.arm().await;
    let started = start(&mut conn, &device, &first).await;
    assert_eq!(started.status, 200, "{}", started.answer);

    let _second = rig.arm().await;
    let (status, answer) = finish(&mut conn, started.key.as_ref().expect("key")).await;
    assert_eq!((status, detail(&answer)), (410, "pake_session_gone"));
    assert!(rig.host.devices.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cli_helper_reads_the_token_and_asks_the_daemon_for_a_code() {
    let rig = Rig::new("pake-cli").await;
    let token = hn_core_host::pake_cli::read_hook_token(&rig.host.data_dir).expect("token");
    assert_eq!(token, rig.host.hook_token);

    let armed = hn_core_host::pake_cli::arm_code(rig.plain, &token)
        .await
        .expect("armed");
    assert_eq!((armed.code.len(), armed.expires_in), (8, 180));
    let (taken, _) = rig
        .ctx
        .pake_slot
        .take()
        .expect("the daemon's slot holds the code");
    assert_eq!(taken.reveal(), armed.code);

    let refused = hn_core_host::pake_cli::arm_code(rig.plain, "wrong")
        .await
        .err()
        .expect("a wrong token is refused")
        .to_string();
    assert!(refused.contains("401"), "{refused}");
    assert!(!refused.contains(&token), "the token is never in an error");

    // A port nothing listens on: a clear error, not a hang.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let closed_addr = closed.local_addr().expect("addr");
    drop(closed);
    let error = hn_core_host::pake_cli::arm_code(closed_addr, &token)
        .await
        .err()
        .expect("no daemon")
        .to_string();
    assert!(error.contains("not running"), "{error}");
}
