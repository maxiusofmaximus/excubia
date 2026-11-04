//! Named secrets. On Windows they sit in the Credential Manager, never in the
//! repository, a log or a plain file. The mail accounts' master key is the
//! one that lives here; the rest of the core keeps its keys in owner-only files.

/// One named secret. Implementations must never log the value.
pub trait SecretStore: Send + Sync {
    fn read(&self) -> std::io::Result<Option<Vec<u8>>>;
    fn write(&self, value: &[u8]) -> std::io::Result<()>;
    /// Forgets the secret; a secret that is already absent is not an error.
    fn delete(&self) -> std::io::Result<()>;
}

#[cfg(windows)]
pub use windows_store::CredentialManager;

#[cfg(windows)]
mod windows_store {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
        CRED_TYPE_GENERIC,
    };

    use super::SecretStore;

    /// A generic credential in the Windows Credential Manager, under `target`.
    pub struct CredentialManager {
        target: Vec<u16>,
    }

    impl CredentialManager {
        /// `target` is the credential's name, for example `excubia/mail-master-key`.
        pub fn new(target: &str) -> Self {
            Self {
                target: target.encode_utf16().chain(std::iter::once(0)).collect(),
            }
        }
    }

    impl SecretStore for CredentialManager {
        fn read(&self) -> std::io::Result<Option<Vec<u8>>> {
            let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
            // SAFETY: `target` is NUL-terminated and lives across the call. On
            // success `credential` is released with CredFree below, exactly once.
            let found =
                unsafe { CredReadW(self.target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
            if found == 0 {
                // SAFETY: reads this thread's last error code; no pointers involved.
                return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                    Ok(None)
                } else {
                    Err(last_error("read"))
                };
            }
            // SAFETY: CredReadW succeeded, so `credential` points to a valid
            // record whose blob is `CredentialBlobSize` bytes until CredFree.
            let blob = unsafe {
                let record = &*credential;
                std::slice::from_raw_parts(
                    record.CredentialBlob,
                    record.CredentialBlobSize as usize,
                )
                .to_vec()
            };
            // SAFETY: `credential` came from CredReadW and is freed once here.
            unsafe { CredFree(credential.cast()) };
            Ok(Some(blob))
        }

        fn write(&self, value: &[u8]) -> std::io::Result<()> {
            let blob_size =
                u32::try_from(value.len()).map_err(|_| std::io::Error::other("value too large"))?;
            let credential = CREDENTIALW {
                Type: CRED_TYPE_GENERIC,
                TargetName: self.target.as_ptr().cast_mut(),
                CredentialBlobSize: blob_size,
                CredentialBlob: value.as_ptr().cast_mut(),
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                ..Default::default()
            };
            // SAFETY: every pointer in `credential` borrows a buffer that outlives
            // the call, and CredWriteW copies what it needs.
            if unsafe { CredWriteW(&credential, 0) } == 0 {
                return Err(last_error("write"));
            }
            Ok(())
        }

        fn delete(&self) -> std::io::Result<()> {
            // SAFETY: `target` is NUL-terminated and lives across the call.
            if unsafe { CredDeleteW(self.target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0 {
                return Ok(());
            }
            // SAFETY: reads this thread's last error code; no pointers involved.
            if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                Ok(())
            } else {
                Err(last_error("delete"))
            }
        }
    }

    fn last_error(action: &str) -> std::io::Error {
        // SAFETY: reads this thread's last error code; no pointers involved.
        let code = unsafe { GetLastError() };
        std::io::Error::other(format!(
            "Credential Manager {action} failed with code {code}"
        ))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_credential_manager_round_trips_a_value() {
            use windows_sys::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

            let target = format!("herdr-notify/persist-test-{}", std::process::id());
            let store = CredentialManager::new(&target);
            assert!(store.read().expect("read before write").is_none());
            store.write(&[1, 2, 3, 4]).expect("write");
            assert_eq!(
                store.read().expect("read after write"),
                Some(vec![1, 2, 3, 4])
            );
            let wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
            // SAFETY: `wide` is NUL-terminated and lives across the call.
            unsafe { CredDeleteW(wide.as_ptr(), CRED_TYPE_GENERIC, 0) };
        }
    }
}
