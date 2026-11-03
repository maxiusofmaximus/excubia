//! The PAKE pairing code lives in memory only: `POST /v1/pair` does not accept it and `devices.json` never holds it.

mod harness;

use harness::TestHost;
use hn_core_api::PAKE_CODE_TTL;
use hn_core_test_support::{pair_device, DemoIdentity};

#[tokio::test(flavor = "multi_thread")]
async fn a_pake_code_does_not_pair_a_device_over_the_plain_route() {
    let host = TestHost::start("pake-slot-pair").await;
    let shown = host.pake_slot.arm(PAKE_CODE_TTL, "host").expect("arm");
    let pake_code = shown.reveal().to_owned();
    let identity = DemoIdentity::generate("", "intruder");

    let refused = pair_device(
        &host.base_url(),
        "p256",
        &identity.public_key_b64(),
        "intruder",
        &pake_code,
    )
    .await
    .expect_err("a PAKE code is not a pairing code");
    assert_eq!(refused.status, 403, "{}", refused.body);
    assert!(host.devices.is_empty(), "no device may be created");

    // Control: the same request with a code from the `codes` store does pair, so the 403 above was the code.
    let code = host.pair_code();
    pair_device(
        &host.base_url(),
        "p256",
        &identity.public_key_b64(),
        "owner",
        &code,
    )
    .await
    .expect("a real pairing code pairs");
    assert_eq!(host.devices.len(), 1);

    // The slot is untouched by all this, and the secret is in no file the store wrote.
    let stored = std::fs::read_to_string(host.devices.path().expect("path")).expect("devices.json");
    assert!(
        !stored.contains(&pake_code),
        "the code reached devices.json"
    );
    let (taken, _) = host.pake_slot.take().expect("still armed");
    assert_eq!(taken.reveal(), pake_code);
}
