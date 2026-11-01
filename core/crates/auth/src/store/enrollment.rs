//! Pairing: devices enrolled with a pairing code or a recovery code, and the
//! minting of those codes.

use hn_core_persist::{fill_random, RandomError};

use super::{Device, DeviceStore, EnrollmentCode};
use crate::codes::{code_hash, mint_enrollment_code, mint_recovery_code};
use crate::curve::Curve;
use crate::error::{EnrollmentRejected, StoreError};
use crate::time::unix_seconds;

impl DeviceStore {
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
            device_id: new_device_id()?,
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
        let code = mint_enrollment_code()?;
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
        // Before the code is spent: a failed draw must not burn it.
        let device_id = new_device_id()?;
        entry.used = true;
        let device = Device {
            device_id,
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
        let code = mint_recovery_code()?;
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
        let rotated = mint_recovery_code()?;
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
}

fn new_device_id() -> Result<String, RandomError> {
    let mut bytes = [0u8; 8];
    fill_random(&mut bytes)?;
    Ok(format!("dev-{}", hex::encode(bytes)))
}
