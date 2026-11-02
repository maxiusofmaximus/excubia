//! Registered devices and live enrollment codes, persisted as JSON.
//!
//! A leaked trust store hands out no secrets: devices are public keys and codes
//! are stored as SHA-256 digests. The file is shared between the daemon and the
//! host-side CLI, so every operation re-reads it when another process rewrote
//! it (`reload_if_changed`) and spent or expired codes are dropped, not
//! resurrected.

mod device;
mod enrollment;
mod file;
mod persist;

pub use device::{Device, Permission, PublicDevice};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::StoreError;
use crate::time::unix_seconds;

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

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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

#[cfg(test)]
mod tests;
