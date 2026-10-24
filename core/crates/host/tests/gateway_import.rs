//! A device paired with the Python gateway moves to this host: its trust file
//! is imported, and the phone re-keys itself with its recovery code, keeping
//! its id and its grants.

mod harness;

use harness::TestHost;
use hn_core_auth::ConflictPolicy;
use hn_core_test_support::{pair_device, DemoIdentity, SignedClient};

/// The gateway's devices, written by its own `DeviceStore` (synthetic data).
const GATEWAY_FILE: &str = include_str!("../../auth/tests/fixtures/gateway-devices.json");
/// The recovery code of `dev-synth-a1` as the gateway printed it.
const RECOVERY_A: &str = "DSRH-RUW6-OZ5U-AKW6";
/// The recovery code of `dev-synth-b2`, which the fixture revoked.
const RECOVERY_B: &str = "MIJK-LHJR-BOR5-CICM";

#[tokio::test(flavor = "multi_thread")]
async fn an_imported_device_re_keys_with_its_recovery_code_and_keeps_its_grants() {
    let host = TestHost::start("gateway-import").await;
    let source = host.data_dir.join("gateway-devices.json");
    std::fs::write(&source, GATEWAY_FILE).expect("write the gateway file");
    let entries = host
        .devices
        .import_gateway(&source, ConflictPolicy::Keep)
        .expect("import");
    assert_eq!(entries.len(), 3);
    assert!(host.devices.get("dev-synth-b2").expect("b").revoked);

    let url = host.base_url();
    let mut moved = DemoIdentity::generate("", "moved phone");
    let public_key = moved.public_key_b64();
    let rekeyed = pair_device(&url, "p256", &public_key, "PC", RECOVERY_A)
        .await
        .expect("re-key with the gateway recovery code");
    assert_eq!(rekeyed.device_id, "dev-synth-a1");
    assert_eq!(
        rekeyed.permissions,
        vec!["read_state", "respond", "send_key"]
    );
    assert_ne!(rekeyed.recovery_code.as_deref(), Some(RECOVERY_A));

    // The phone signs with its new key and is served under the grants it had.
    moved.set_device_id(&rekeyed.device_id);
    SignedClient::new(&url, moved)
        .state()
        .await
        .expect("a signed read with read_state");

    // The rotation spent the old code.
    let spent = pair_device(&url, "p256", &public_key, "PC", RECOVERY_A)
        .await
        .expect_err("spent code");
    assert_eq!(spent.status, 403, "{}", spent.body);

    // A revoked device's code is refused.
    let revoked = pair_device(&url, "p256", &public_key, "PC", RECOVERY_B)
        .await
        .expect_err("revoked code");
    assert_eq!(revoked.status, 403, "{}", revoked.body);
    assert!(
        revoked.body.contains("pairing_rejected"),
        "{}",
        revoked.body
    );
}
