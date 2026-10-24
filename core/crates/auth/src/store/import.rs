//! Importing the devices of a gateway trust store (the Python implementation's
//! `devices.json`) into this one.
//!
//! The file has the same shape as ours, so every entry is read as a
//! [`DeviceRecord`] and vetted on its own. An imported device keeps what makes
//! it the same device to its owner: its id, its grants, its revocation and its
//! recovery code hash, so `POST /v1/pair` with that recovery code re-keys it
//! instead of pairing a stranger. Two things are not carried over: the FCM
//! token (the phone registers again) and the file's pairing codes (single use,
//! and expired within minutes).

use std::collections::BTreeMap;
use std::path::Path;

use super::file::{self, DeviceRecord, RecordError};
use super::{Device, DeviceStore, Permission};
use crate::curve::is_valid_public_key;
use crate::error::StoreError;

/// What to do when a gateway device arrives under an id already registered with
/// another key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// Keep the registered device and report the conflict.
    Keep,
    /// Replace the registered device with the imported one.
    Replace,
}

/// What the import did with one entry of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportOutcome {
    /// New here: registered with the file's grants and revocation.
    Added,
    /// Already here with the same key: left exactly as it is.
    AlreadyPresent,
    /// The same key is registered under another id: not registered twice.
    DuplicateKey { known_as: String },
    /// The id is here with another key. Kept, unless the import was forced.
    KeyConflict,
    /// The id is here with another key and the import was forced: replaced.
    Replaced,
    /// The entry names no usable device, so nothing is written for it.
    Rejected(RecordError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportEntry {
    pub device_id: String,
    /// Not a secret: it is what tells two devices apart in the report.
    pub label: Option<String>,
    pub outcome: ImportOutcome,
    /// Permission labels this host does not know. They are dropped and the
    /// device is kept with the rest of its grants: the gateway drops unknown
    /// labels on load for the same reason, and a grant this host cannot name
    /// must not cost the operator the device.
    pub unknown_permissions: Vec<String>,
}

impl DeviceStore {
    /// Merge the devices of a gateway trust store into this one.
    ///
    /// Entries are judged in file order against the store as it grows. The
    /// merged set is written once, atomically; if that write fails the store
    /// keeps the devices it had. The source file is only read.
    pub fn import_gateway(
        &self,
        path: &Path,
        policy: ConflictPolicy,
    ) -> Result<Vec<ImportEntry>, StoreError> {
        let trust_file = file::read(path)?;
        let mut inner = self.lock();
        let previous = inner.devices.clone();
        let mut changed = false;
        let mut entries = Vec::with_capacity(trust_file.devices.len());
        for record in trust_file.devices {
            let (entry, device) = judge(record, &inner.devices, policy);
            if let Some(device) = device {
                changed = true;
                inner.devices.insert(device.device_id.clone(), device);
            }
            entries.push(entry);
        }
        if changed {
            if let Err(error) = self.save(&inner) {
                inner.devices = previous;
                return Err(error);
            }
        }
        Ok(entries)
    }
}

/// Decide one entry against the devices already here. Returns the entry for
/// the report and the device to write, if any.
fn judge(
    record: DeviceRecord,
    known: &BTreeMap<String, Device>,
    policy: ConflictPolicy,
) -> (ImportEntry, Option<Device>) {
    let device_id = record.device_id.clone();
    let label = record.label.clone();
    let unknown_permissions = unknown_permissions(&record);
    let (outcome, device) = match vet(record) {
        Ok(device) => decide(device, known, policy),
        Err(error) => (ImportOutcome::Rejected(error), None),
    };
    let entry = ImportEntry {
        device_id,
        label,
        outcome,
        unknown_permissions,
    };
    (entry, device)
}

/// A record as an importable device: the conversion the store applies on load,
/// plus a key check. A key this host cannot verify with is no device at all.
fn vet(record: DeviceRecord) -> Result<Device, RecordError> {
    let device = record.into_device()?;
    if !is_valid_public_key(device.curve, &device.public_key) {
        return Err(RecordError::InvalidPublicKey);
    }
    Ok(Device {
        fcm_token: None,
        persisted_at: None,
        ..device
    })
}

fn decide(
    device: Device,
    known: &BTreeMap<String, Device>,
    policy: ConflictPolicy,
) -> (ImportOutcome, Option<Device>) {
    if let Some(existing) = known.get(&device.device_id) {
        if existing.public_key == device.public_key {
            return (ImportOutcome::AlreadyPresent, None);
        }
        return match policy {
            ConflictPolicy::Keep => (ImportOutcome::KeyConflict, None),
            ConflictPolicy::Replace => (ImportOutcome::Replaced, Some(device)),
        };
    }
    match known
        .values()
        .find(|existing| existing.public_key == device.public_key)
    {
        Some(existing) => (
            ImportOutcome::DuplicateKey {
                known_as: existing.device_id.clone(),
            },
            None,
        ),
        None => (ImportOutcome::Added, Some(device)),
    }
}

fn unknown_permissions(record: &DeviceRecord) -> Vec<String> {
    record
        .permissions
        .iter()
        .flatten()
        .filter(|label| Permission::from_label(label.as_str()).is_none())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::code_hash;
    use hn_core_test_dir::TestDir;

    /// Generated by the gateway's own `DeviceStore` (synthetic devices, see
    /// `crates/auth/tests/fixtures/`).
    const FIXTURE: &str = include_str!("../../tests/fixtures/gateway-devices.json");
    /// The recovery code of `dev-synth-a1` as the gateway printed it.
    const RECOVERY_A: &str = "DSRH-RUW6-OZ5U-AKW6";
    /// The public key of `dev-synth-c3` in the fixture.
    const KEY_C: &str = "7EFpCp8Zjqg4DafDxhjTG/ugGwJxKwVe/5FxsghLyRE=";

    fn scratch(name: &str) -> (DeviceStore, TestDir) {
        let dir = TestDir::new(&format!("auth-import-{name}"));
        let store = DeviceStore::open(Some(dir.join("devices.json")));
        (store, dir)
    }

    fn source(dir: &TestDir, content: &str) -> std::path::PathBuf {
        let path = dir.join("gateway-devices.json");
        std::fs::write(&path, content).expect("write the gateway file");
        path
    }

    fn outcomes(entries: &[ImportEntry]) -> Vec<ImportOutcome> {
        entries.iter().map(|entry| entry.outcome.clone()).collect()
    }

    fn one_device(device: &str) -> String {
        format!(r#"{{"version": 1, "devices": [{device}]}}"#)
    }

    #[test]
    fn the_gateway_fixture_imports_with_its_identity_intact() {
        let (store, dir) = scratch("fixture");
        let entries = store
            .import_gateway(&source(&dir, FIXTURE), ConflictPolicy::Keep)
            .expect("import");
        assert_eq!(
            outcomes(&entries),
            vec![ImportOutcome::Added; 3],
            "all three devices are new"
        );

        let a = store.get("dev-synth-a1").expect("device a");
        assert_eq!(
            a.permissions,
            vec![
                Permission::ReadState,
                Permission::Respond,
                Permission::SendKey
            ]
        );
        assert_eq!(
            a.recovery_code_hash.as_deref(),
            Some(code_hash(RECOVERY_A).as_str()),
            "the gateway's code hash is the one this host computes"
        );

        let b = store.get("dev-synth-b2").expect("device b");
        assert!(b.revoked);
        assert!(store.get_trusted("dev-synth-b2").is_none());

        let c = store.get("dev-synth-c3").expect("device c");
        assert_eq!(c.permissions, vec![Permission::Admin]);
        assert!(c.recovery_code_hash.is_none());

        let written = file::read(&dir.join("devices.json")).expect("read the store");
        assert!(written.codes.is_empty(), "pairing codes are not imported");
        assert!(written
            .devices
            .iter()
            .all(|record| record.fcm_token.is_none()));
    }

    #[test]
    fn reimporting_changes_nothing() {
        let (store, dir) = scratch("idempotent");
        let path = source(&dir, FIXTURE);
        store
            .import_gateway(&path, ConflictPolicy::Keep)
            .expect("first import");
        let before = std::fs::read_to_string(dir.join("devices.json")).expect("read");

        let entries = store
            .import_gateway(&path, ConflictPolicy::Keep)
            .expect("second import");
        assert_eq!(outcomes(&entries), vec![ImportOutcome::AlreadyPresent; 3]);
        assert_eq!(store.len(), 3);
        let after = std::fs::read_to_string(dir.join("devices.json")).expect("read");
        assert_eq!(before, after, "nothing new means nothing written");
    }

    #[test]
    fn a_key_conflict_is_kept_unless_forced() {
        let (store, dir) = scratch("conflict");
        let path = source(&dir, FIXTURE);
        store
            .import_gateway(&path, ConflictPolicy::Keep)
            .expect("first import");
        let original = store.get("dev-synth-a1").expect("a").public_key;

        let other_key = source(
            &dir,
            &one_device(&format!(
                r#"{{"device_id": "dev-synth-a1", "curve": "ed25519", "public_key": "{KEY_C}"}}"#
            )),
        );
        let entries = store
            .import_gateway(&other_key, ConflictPolicy::Keep)
            .expect("conflict");
        assert_eq!(outcomes(&entries), vec![ImportOutcome::KeyConflict]);
        assert_eq!(store.get("dev-synth-a1").expect("a").public_key, original);

        let entries = store
            .import_gateway(&other_key, ConflictPolicy::Replace)
            .expect("forced");
        assert_eq!(outcomes(&entries), vec![ImportOutcome::Replaced]);
        assert_eq!(store.get("dev-synth-a1").expect("a").public_key, KEY_C);
    }

    #[test]
    fn a_key_already_registered_under_another_id_is_not_duplicated() {
        let (store, dir) = scratch("duplicate");
        store
            .import_gateway(&source(&dir, FIXTURE), ConflictPolicy::Keep)
            .expect("first import");
        let again = source(
            &dir,
            &one_device(&format!(
                r#"{{"device_id": "dev-other", "curve": "ed25519", "public_key": "{KEY_C}"}}"#
            )),
        );
        let entries = store
            .import_gateway(&again, ConflictPolicy::Replace)
            .expect("import");
        assert_eq!(
            outcomes(&entries),
            vec![ImportOutcome::DuplicateKey {
                known_as: "dev-synth-c3".into()
            }]
        );
        assert_eq!(store.len(), 3);
    }

    #[test]
    fn corrupt_entries_are_reported_and_the_rest_still_imports() {
        let (store, dir) = scratch("corrupt");
        let content = format!(
            r#"{{"version": 1, "devices": [
                {{"device_id": "dev-no-key", "curve": "p256", "public_key": ""}},
                {{"device_id": "", "curve": "p256", "public_key": "AAAA"}},
                {{"device_id": "dev-curve", "curve": "rsa", "public_key": "AAAA"}},
                {{"device_id": "dev-point", "curve": "p256", "public_key": "AAAA"}},
                {{"device_id": "dev-base64", "curve": "ed25519", "public_key": "no-base64!"}},
                {{"device_id": "dev-good", "curve": "ed25519", "public_key": "{KEY_C}"}}
            ]}}"#
        );
        let entries = store
            .import_gateway(&source(&dir, &content), ConflictPolicy::Keep)
            .expect("import");
        assert_eq!(
            outcomes(&entries),
            vec![
                ImportOutcome::Rejected(RecordError::MissingPublicKey),
                ImportOutcome::Rejected(RecordError::MissingDeviceId),
                ImportOutcome::Rejected(RecordError::UnsupportedCurve),
                ImportOutcome::Rejected(RecordError::InvalidPublicKey),
                ImportOutcome::Rejected(RecordError::InvalidPublicKey),
                ImportOutcome::Added,
            ]
        );
        assert_eq!(store.len(), 1);
        assert!(store.get("dev-good").is_some());
    }

    #[test]
    fn an_unknown_permission_is_dropped_and_the_device_kept() {
        let (store, dir) = scratch("permission");
        let content = one_device(&format!(
            r#"{{"device_id": "dev-perm", "curve": "ed25519", "public_key": "{KEY_C}",
                "permissions": ["respond", "telepathy"]}}"#
        ));
        let entries = store
            .import_gateway(&source(&dir, &content), ConflictPolicy::Keep)
            .expect("import");
        assert_eq!(outcomes(&entries), vec![ImportOutcome::Added]);
        assert_eq!(
            entries[0].unknown_permissions,
            vec!["telepathy".to_string()]
        );
        let device = store.get("dev-perm").expect("device");
        assert_eq!(device.permissions, vec![Permission::Respond]);
    }

    #[test]
    fn a_missing_file_is_an_error_and_writes_nothing() {
        let (store, dir) = scratch("missing");
        let error = store
            .import_gateway(&dir.join("nope.json"), ConflictPolicy::Keep)
            .expect_err("no file");
        assert!(matches!(error, StoreError::Io(_)), "{error:?}");
        assert!(store.is_empty());
        assert!(!dir.join("devices.json").exists());
    }

    #[test]
    fn a_newer_store_version_is_refused() {
        let (store, dir) = scratch("version");
        let error = store
            .import_gateway(
                &source(&dir, r#"{"version": 2, "devices": []}"#),
                ConflictPolicy::Keep,
            )
            .expect_err("version");
        assert!(matches!(error, StoreError::Version(2)), "{error:?}");
    }
}
