//! Store tests: enrollment lifecycle, revocation, permissions, persistence.
use super::*;
use hn_core_test_dir::TestDir;

/// The store, its file and the scratch directory that holds both.
fn temp_store(name: &str) -> (DeviceStore, PathBuf, TestDir) {
    let dir = TestDir::new(&format!("auth-tests-{name}"));
    let path = dir.join("devices.json");
    let store = DeviceStore::open(Some(path.clone()));
    (store, path, dir)
}

fn public_key() -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    STANDARD.encode([4u8; 65])
}

#[test]
fn enrollment_codes_are_single_use_and_persisted_hashed() {
    let (store, path, _dir) = temp_store("single-use");
    let code = store.issue_enrollment_code(600.0).expect("issue");
    let raw = std::fs::read_to_string(&path).expect("read file");
    assert!(
        !raw.contains(&code),
        "the plaintext code must never be written"
    );
    assert!(raw.contains("code_hash"));

    let device = store
        .enroll(&code, Curve::P256, &public_key(), Some("PC".into()), "")
        .expect("enroll");
    assert!(
        device.permissions.is_empty(),
        "a new device is pending until the host grants it (CORE11, D3)"
    );

    // A second process (the daemon) sees the spent code as gone.
    let reopened = DeviceStore::open(Some(path.clone()));
    assert!(reopened
        .enroll(&code, Curve::P256, &public_key(), None, "")
        .is_err());
    assert_eq!(reopened.len(), 1);
}

#[test]
fn expired_codes_are_refused() {
    let (store, _, _dir) = temp_store("expired");
    let code = store.issue_enrollment_code(0.0).expect("issue");
    std::thread::sleep(std::time::Duration::from_millis(10));
    assert!(store
        .enroll(&code, Curve::P256, &public_key(), None, "")
        .is_err());
}

#[test]
fn a_malformed_request_does_not_burn_the_code() {
    let (store, _, _dir) = temp_store("malformed");
    let code = store.issue_enrollment_code(600.0).expect("issue");
    assert!(store.enroll(&code, Curve::P256, "", None, "").is_err());
    assert!(store
        .enroll(&code, Curve::P256, &public_key(), None, "")
        .is_ok());
}

#[test]
fn recovery_codes_rekey_and_rotate() {
    let (store, _, _dir) = temp_store("recovery");
    let code = store.issue_enrollment_code(600.0).expect("issue");
    let device = store
        .enroll(&code, Curve::P256, &public_key(), None, "")
        .expect("enroll");
    let recovery = store
        .issue_recovery_code(&device.device_id)
        .expect("issue")
        .expect("some");

    let new_key = public_key();
    let (rekeyed, rotated) = store
        .use_recovery_code(
            &recovery,
            Curve::Ed25519,
            &new_key,
            Some("nuevo".into()),
            "",
        )
        .expect("rekey");
    assert_eq!(rekeyed.device_id, device.device_id);
    assert_eq!(rekeyed.curve, Curve::Ed25519);
    assert_eq!(rekeyed.label.as_deref(), Some("nuevo"));
    assert_ne!(rotated, recovery);
    // The old code is spent by the rotation.
    assert!(store
        .use_recovery_code(&recovery, Curve::P256, &new_key, None, "")
        .is_err());
}

#[test]
fn revoked_devices_are_marked_and_their_recovery_refused() {
    let (store, path, _dir) = temp_store("revoked");
    let code = store.issue_enrollment_code(600.0).expect("issue");
    let device = store
        .enroll(&code, Curve::P256, &public_key(), None, "")
        .expect("enroll");
    let recovery = store
        .issue_recovery_code(&device.device_id)
        .expect("issue")
        .expect("some");
    assert!(store.revoke(&device.device_id).expect("revoke"));
    assert!(store.get_trusted(&device.device_id).is_none());
    assert!(store
        .use_recovery_code(&recovery, Curve::P256, &public_key(), None, "")
        .is_err());

    let reopened = DeviceStore::open(Some(path));
    let record = reopened.get(&device.device_id).expect("still listed");
    assert!(record.revoked, "revocation survives a reload");
}

#[test]
fn a_corrupt_store_trusts_nobody() {
    let (_, path, _dir) = temp_store("corrupt");
    std::fs::write(&path, "{ not json").expect("write");
    let store = DeviceStore::open(Some(path));
    assert_eq!(store.len(), 0);
    assert!(store.get_trusted("dev-anything").is_none());
}

#[test]
fn permissions_survive_a_round_trip_and_unknown_ids_are_dropped() {
    let (store, path, _dir) = temp_store("permissions");
    let code = store.issue_enrollment_code(600.0).expect("issue");
    let device = store
        .enroll(&code, Curve::P256, &public_key(), None, "")
        .expect("enroll");
    assert!(store
        .grant(&device.device_id, Permission::Respond)
        .expect("grant"));

    // An unknown permission id in the file grants nothing.
    let raw = std::fs::read_to_string(&path).expect("read");
    let tampered = raw.replace("\"respond\"", "\"respond\", \"superuser\"");
    std::fs::write(&path, tampered).expect("write");
    let reopened = DeviceStore::open(Some(path));
    let record = reopened.get(&device.device_id).expect("device");
    assert!(record.has(Permission::Respond));
    assert_eq!(
        record.permissions.len(),
        1,
        "unknown ids are dropped; a pending device holds only what the host granted (CORE11, D3)"
    );
}

#[test]
fn touch_persists_at_most_once_per_interval() {
    let (store, path, _dir) = temp_store("touch");
    let code = store.issue_enrollment_code(600.0).expect("issue");
    let device = store
        .enroll(&code, Curve::P256, &public_key(), None, "")
        .expect("enroll");

    store.touch(&device.device_id).expect("touch");
    let first = std::fs::read_to_string(&path).expect("read");
    store.touch(&device.device_id).expect("touch again");
    let second = std::fs::read_to_string(&path).expect("read");
    assert_eq!(first, second, "the second touch stays in memory");
}
