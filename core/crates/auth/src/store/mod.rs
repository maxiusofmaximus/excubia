//! Registered devices and live enrollment codes, persisted as JSON.
//!
//! A leaked trust store hands out no secrets: devices are public keys and codes
//! are stored as SHA-256 digests. The file is shared between the daemon and the
//! host-side CLI, so every operation re-reads it when another process rewrote
//! it (`reload_if_changed`) and spent or expired codes are dropped, not
//! resurrected.

mod device;
mod file;
mod import;

pub use device::{Device, Permission, PublicDevice};
pub use file::RecordError;
pub use import::{ConflictPolicy, ImportEntry, ImportOutcome};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rand_core::{OsRng, RngCore};

use crate::codes::{code_hash, mint_enrollment_code, mint_recovery_code};
use crate::curve::Curve;
use crate::error::{EnrollmentRejected, StoreError};
use crate::time::unix_seconds;
use file::{CodeRecord, TrustFile};

/// Minimum seconds between two persisted `last_seen_at` writes.
const TOUCH_PERSIST_INTERVAL_SECONDS: f64 = 60.0;

struct EnrollmentCode {
    created_at: f64,
    expires_at: f64,
    used: bool,
}

impl EnrollmentCode {
    fn valid_at(&self, now: f64) -> bool {
        !self.used && now < self.expires_at
    }
}

struct Inner {
    devices: BTreeMap<String, Device>,
    codes: BTreeMap<String, EnrollmentCode>,
    stamp: Option<std::time::SystemTime>,
}

pub struct DeviceStore {
    path: Option<PathBuf>,
    inner: Mutex<Inner>,
}

impl DeviceStore {
    /// Open a store. A file that cannot be read leaves the store empty and
    /// trusts nobody â€” it is never treated as "no devices, so allow all".
    pub fn open(path: Option<PathBuf>) -> Self {
        let store = Self {
            path,
            inner: Mutex::new(Inner {
                devices: BTreeMap::new(),
                codes: BTreeMap::new(),
                stamp: None,
            }),
        };
        store.reload();
        store
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn len(&self) -> usize {
        self.lock().devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn list(&self) -> Vec<Device> {
        let mut devices: Vec<Device> = self.lock().devices.values().cloned().collect();
        devices.sort_by(|a, b| a.created_at.total_cmp(&b.created_at));
        devices
    }

    pub fn get(&self, device_id: &str) -> Option<Device> {
        self.lock().devices.get(device_id).cloned()
    }

    /// The device only when it exists and is not revoked.
    pub fn get_trusted(&self, device_id: &str) -> Option<Device> {
        self.get(device_id).filter(|device| !device.revoked)
    }

    /// Re-read the store when another process (the CLI) rewrote it.
    pub fn reload_if_changed(&self) -> bool {
        let stamp = self.file_stamp();
        if stamp == self.lock().stamp {
            return false;
        }
        self.reload();
        true
    }

    fn reload(&self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let stamp = self.file_stamp();
        if !path.is_file() {
            let mut inner = self.lock();
            inner.stamp = stamp;
            return;
        }
        match file::read(&path) {
            Ok(file) => {
                let now = unix_seconds();
                let devices: BTreeMap<String, Device> = file
                    .devices
                    .into_iter()
                    .filter_map(|record| record.into_device().ok())
                    .map(|device| (device.device_id.clone(), device))
                    .collect();
                let codes: BTreeMap<String, EnrollmentCode> = file
                    .codes
                    .into_iter()
                    .filter(|record| !record.code_hash.is_empty())
                    .map(|record| {
                        (
                            record.code_hash.clone(),
                            EnrollmentCode {
                                created_at: record.created_at,
                                expires_at: record.expires_at,
                                used: false,
                            },
                        )
                    })
                    .filter(|(_, code)| code.valid_at(now))
                    .collect();
                let mut inner = self.lock();
                inner.devices = devices;
                inner.codes = codes;
                inner.stamp = stamp;
            }
            Err(error) => {
                // Keep the last good state: a transiently unreadable file must
                // not silently un-pair every device.
                eprintln!("auth: trust store unreadable, keeping previous state: {error}");
            }
        }
    }

    fn file_stamp(&self) -> Option<std::time::SystemTime> {
        self.path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).ok())
            .and_then(|metadata| metadata.modified().ok())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn save(&self, inner: &Inner) -> Result<(), StoreError> {
        let Some(path) = self.path.as_ref() else {
            return Ok(());
        };
        let now = unix_seconds();
        let devices = inner
            .devices
            .values()
            .map(|device| device.to_record())
            .collect();
        // Spent and expired codes are dropped, so single use survives a
        // restart; only live codes are written.
        let codes = inner
            .codes
            .iter()
            .filter(|(_, code)| code.valid_at(now))
            .map(|(hash, code)| CodeRecord {
                code_hash: hash.clone(),
                created_at: code.created_at,
                expires_at: code.expires_at,
            })
            .collect();
        file::write(
            path,
            &TrustFile {
                version: file::VERSION,
                devices,
                codes,
            },
        )
    }

    /// Register a device without a code (host-side act or owner auto-approval).
    /// The device is pending: it holds no permission until the host grants one
    /// (CORE11, D3), so a pairing never reads state on its own.
    pub fn add_device(
        &self,
        curve: Curve,
        public_key: &str,
        label: Option<String>,
        owner_id: &str,
    ) -> Result<Device, StoreError> {
        let now = unix_seconds();
        let mut inner = self.lock();
        let device = Device {
            device_id: new_device_id(),
            curve,
            public_key: public_key.to_string(),
            label,
            created_at: now,
            last_seen_at: None,
            revoked: false,
            permissions: Vec::new(),
            fcm_token: None,
            owner_id: owner_id.to_string(),
            recovery_code_hash: None,
            persisted_at: Some(now),
        };
        inner
            .devices
            .insert(device.device_id.clone(), device.clone());
        self.save(&inner)?;
        Ok(device)
    }

    /// Mint a single-use pairing code and persist its hash. The plaintext is
    /// returned once, for the operator to read off the host.
    pub fn issue_enrollment_code(&self, ttl_seconds: f64) -> Result<String, StoreError> {
        let now = unix_seconds();
        let code = mint_enrollment_code();
        let mut inner = self.lock();
        inner.codes.retain(|_, entry| entry.valid_at(now));
        inner.codes.insert(
            code_hash(&code),
            EnrollmentCode {
                created_at: now,
                expires_at: now + ttl_seconds,
                used: false,
            },
        );
        self.save(&inner)?;
        Ok(code)
    }

    /// Register a device against a live pairing code. The request is validated
    /// before the code is spent, so a malformed request cannot burn a code the
    /// operator just printed.
    pub fn enroll(
        &self,
        code: &str,
        curve: Curve,
        public_key: &str,
        label: Option<String>,
        owner_id: &str,
    ) -> Result<Device, EnrollmentRejected> {
        if code.is_empty() {
            return Err(EnrollmentRejected::new("enrollment code is required"));
        }
        let now = unix_seconds();
        let mut inner = self.lock();
        let hash = code_hash(code);
        let Some(entry) = inner.codes.get_mut(&hash) else {
            inner.codes.retain(|_, entry| entry.valid_at(now));
            return Err(EnrollmentRejected::new(
                "enrollment code is unknown, expired, or used",
            ));
        };
        if !entry.valid_at(now) {
            inner.codes.retain(|_, entry| entry.valid_at(now));
            return Err(EnrollmentRejected::new(
                "enrollment code is unknown, expired, or used",
            ));
        }
        if public_key.is_empty() {
            return Err(EnrollmentRejected::new("public_key is required"));
        }
        entry.used = true;
        let device = Device {
            device_id: new_device_id(),
            curve,
            public_key: public_key.to_string(),
            label,
            created_at: now,
            last_seen_at: None,
            revoked: false,
            permissions: Vec::new(),
            fcm_token: None,
            owner_id: owner_id.to_string(),
            recovery_code_hash: None,
            persisted_at: Some(now),
        };
        inner
            .devices
            .insert(device.device_id.clone(), device.clone());
        self.save(&inner)
            .map_err(|error| EnrollmentRejected::new(format!("store write failed: {error}")))?;
        Ok(device)
    }

    /// Mint (and rotate) a device's recovery code. Returned once; only the
    /// hash is stored.
    pub fn issue_recovery_code(&self, device_id: &str) -> Result<Option<String>, StoreError> {
        let code = mint_recovery_code();
        let mut inner = self.lock();
        let Some(device) = inner.devices.get_mut(device_id) else {
            return Ok(None);
        };
        device.recovery_code_hash = Some(code_hash(&code));
        self.save(&inner)?;
        Ok(Some(code))
    }

    /// Re-key the device that owns `code`; a revoked device's code is refused.
    pub fn use_recovery_code(
        &self,
        code: &str,
        curve: Curve,
        public_key: &str,
        label: Option<String>,
        owner_id: &str,
    ) -> Result<(Device, String), EnrollmentRejected> {
        if code.is_empty() {
            return Err(EnrollmentRejected::new("recovery code is required"));
        }
        if public_key.is_empty() {
            return Err(EnrollmentRejected::new("public_key is required"));
        }
        let digest = code_hash(code);
        let mut inner = self.lock();
        let Some(device_id) = inner
            .devices
            .values()
            .find(|device| {
                device.recovery_code_hash.as_deref() == Some(digest.as_str()) && !device.revoked
            })
            .map(|device| device.device_id.clone())
        else {
            return Err(EnrollmentRejected::new(
                "recovery code is unknown or the device is revoked",
            ));
        };
        let rotated = mint_recovery_code();
        let device = inner.devices.get_mut(&device_id).expect("device exists");
        device.curve = curve;
        device.public_key = public_key.to_string();
        if label.is_some() {
            device.label = label;
        }
        if !owner_id.is_empty() {
            device.owner_id = owner_id.to_string();
        }
        device.recovery_code_hash = Some(code_hash(&rotated));
        let device = device.clone();
        self.save(&inner)
            .map_err(|error| EnrollmentRejected::new(format!("store write failed: {error}")))?;
        Ok((device, rotated))
    }

    /// Revocation marks; it never deletes, so a revoked device stays auditable.
    pub fn revoke(&self, device_id: &str) -> Result<bool, StoreError> {
        let mut inner = self.lock();
        let Some(device) = inner.devices.get_mut(device_id) else {
            return Ok(false);
        };
        device.revoked = true;
        device.fcm_token = None;
        self.save(&inner)?;
        Ok(true)
    }

    /// Host-side grant: widening access is an act performed at the machine.
    pub fn grant(&self, device_id: &str, permission: Permission) -> Result<bool, StoreError> {
        let mut inner = self.lock();
        let Some(device) = inner.devices.get_mut(device_id) else {
            return Ok(false);
        };
        if !device.has(permission) {
            device.permissions.push(permission);
        }
        self.save(&inner)?;
        Ok(true)
    }

    /// Record that a device was just seen. The in-memory stamp is always
    /// current; the file is rewritten at most once per interval.
    pub fn touch(&self, device_id: &str) -> Result<(), StoreError> {
        let now = unix_seconds();
        let mut inner = self.lock();
        let Some(device) = inner.devices.get_mut(device_id) else {
            return Ok(());
        };
        device.last_seen_at = Some(now);
        let previous = device.persisted_at;
        if previous.is_some_and(|previous| now - previous < TOUCH_PERSIST_INTERVAL_SECONDS) {
            return Ok(());
        }
        device.persisted_at = Some(now);
        self.save(&inner)
    }
}

fn new_device_id() -> String {
    let mut bytes = [0u8; 8];
    OsRng.fill_bytes(&mut bytes);
    format!("dev-{}", hex::encode(bytes))
}

#[cfg(test)]
mod tests;
