//! The trust file: re-read when another process rewrote it, and written after
//! every change. A file that cannot be read keeps the last good state.

use std::collections::BTreeMap;

use super::file::{self, CodeRecord, TrustFile};
use super::{Device, DeviceStore, EnrollmentCode, Inner};
use crate::error::StoreError;
use crate::time::unix_seconds;

impl DeviceStore {
    /// Re-read the store when another process (the CLI) rewrote it.
    pub fn reload_if_changed(&self) -> bool {
        let stamp = self.file_stamp();
        if stamp == self.lock().stamp {
            return false;
        }
        self.reload();
        true
    }

    pub(super) fn reload(&self) {
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

    pub(super) fn save(&self, inner: &Inner) -> Result<(), StoreError> {
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
}
