//! The trust-store file: `{version, devices, codes}` pretty JSON, written
//! atomically (tmp + rename) and read fail-closed.
//!
//! The shape is the gateway's (`gateway/src/herdr_notify/api/auth/devices.py`),
//! so a store written by one implementation can be read by the other. Entries
//! that do not parse are dropped individually, never the whole file.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::curve::Curve;
use crate::error::StoreError;
use crate::store::device::{Device, Permission};

pub const VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct TrustFile {
    pub version: u32,
    #[serde(default)]
    pub devices: Vec<DeviceRecord>,
    #[serde(default)]
    pub codes: Vec<CodeRecord>,
}

/// The on-disk device record. `permissions` is optional on purpose: a missing
/// field is a pre-M2 record and receives the pairing default, while an
/// explicitly stored empty set stays empty.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DeviceRecord {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub curve: String,
    #[serde(default)]
    pub public_key: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub created_at: f64,
    #[serde(default)]
    pub last_seen_at: Option<f64>,
    #[serde(default)]
    pub revoked: bool,
    #[serde(default)]
    pub permissions: Option<Vec<String>>,
    #[serde(default)]
    pub fcm_token: Option<String>,
    #[serde(default)]
    pub owner_id: String,
    #[serde(default)]
    pub recovery_code_hash: Option<String>,
    #[serde(default)]
    pub persisted_at: Option<f64>,
}

impl DeviceRecord {
    /// A record that names no usable device is dropped, not guessed at.
    pub fn into_device(self) -> Option<Device> {
        if self.device_id.is_empty() || self.public_key.is_empty() {
            return None;
        }
        let curve = Curve::from_label(&self.curve)?;
        let permissions = match self.permissions {
            None => vec![Permission::ReadState],
            Some(labels) => labels
                .iter()
                .filter_map(|label| Permission::from_label(label))
                .collect(),
        };
        Some(Device {
            device_id: self.device_id,
            curve,
            public_key: self.public_key,
            label: self.label,
            created_at: self.created_at,
            last_seen_at: self.last_seen_at,
            revoked: self.revoked,
            permissions,
            fcm_token: self.fcm_token,
            owner_id: self.owner_id,
            recovery_code_hash: self.recovery_code_hash,
            persisted_at: self.persisted_at,
        })
    }
}

impl Device {
    pub fn to_record(&self) -> DeviceRecord {
        DeviceRecord {
            device_id: self.device_id.clone(),
            curve: self.curve.label().to_string(),
            public_key: self.public_key.clone(),
            label: self.label.clone(),
            created_at: self.created_at,
            last_seen_at: self.last_seen_at,
            revoked: self.revoked,
            permissions: Some(
                self.permissions
                    .iter()
                    .map(|permission| permission.label().to_string())
                    .collect(),
            ),
            fcm_token: self.fcm_token.clone(),
            owner_id: self.owner_id.clone(),
            recovery_code_hash: self.recovery_code_hash.clone(),
            persisted_at: self.persisted_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CodeRecord {
    #[serde(default)]
    pub code_hash: String,
    #[serde(default)]
    pub created_at: f64,
    #[serde(default)]
    pub expires_at: f64,
}

/// Read a store. Every failure is the caller's to turn into "trust nobody".
pub fn read(path: &Path) -> Result<TrustFile, StoreError> {
    let raw = fs::read_to_string(path)?;
    let file: TrustFile = serde_json::from_str(&raw)?;
    if file.version > VERSION {
        return Err(StoreError::Version(file.version));
    }
    Ok(file)
}

/// Write a store atomically: a half-written file must never be believed.
pub fn write(path: &Path, file: &TrustFile) -> Result<(), StoreError> {
    hn_core_persist::write_json_atomic(path, file)?;
    Ok(())
}
