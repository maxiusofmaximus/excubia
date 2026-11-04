//! Moves the device identity from the keyring entry of the old name
//! (`dev.herdrnotify.desktop`) to the new one. Losing it would mean pairing
//! every device again, so the old entry is forgotten only after the new one
//! reads back the same key.

use herdr_desktop_core::identity::{DeviceIdentity, IdentityStore};
use herdr_desktop_core::CoreError;

/// Reads the current entry; when it is empty, adopts the legacy one. Any
/// failed step keeps the legacy entry and uses it for this run.
pub struct MigratingStore<'a> {
    pub current: &'a dyn IdentityStore,
    pub legacy: &'a dyn IdentityStore,
}

impl IdentityStore for MigratingStore<'_> {
    fn load(&self) -> Result<Option<DeviceIdentity>, CoreError> {
        if let Some(identity) = self.current.load()? {
            return Ok(Some(identity));
        }
        let Some(old) = self.legacy.load()? else {
            return Ok(None);
        };
        if self.current.save(&old).is_err() {
            return Ok(Some(old));
        }
        let copied = matches!(self.current.load(), Ok(Some(read)) if same_key(&read, &old));
        if !copied {
            let _ = self.current.remove();
            return Ok(Some(old));
        }
        let _ = self.legacy.remove();
        Ok(Some(old))
    }

    fn save(&self, identity: &DeviceIdentity) -> Result<(), CoreError> {
        self.current.save(identity)
    }

    fn remove(&self) -> Result<(), CoreError> {
        self.current.remove()
    }
}

fn same_key(a: &DeviceIdentity, b: &DeviceIdentity) -> bool {
    a.device_id() == b.device_id() && a.public_key_b64() == b.public_key_b64()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// An in-memory entry whose write can be refused or stored as another key.
    #[derive(Default)]
    struct Entry {
        value: Mutex<Option<DeviceIdentity>>,
        refuse_save: bool,
        corrupt_save: bool,
    }

    impl Entry {
        fn holding(identity: &DeviceIdentity) -> Self {
            Self {
                value: Mutex::new(Some(identity.clone())),
                ..Self::default()
            }
        }

        fn now(&self) -> Option<String> {
            self.value
                .lock()
                .expect("lock")
                .as_ref()
                .map(|identity| identity.device_id().to_string())
        }
    }

    impl IdentityStore for Entry {
        fn load(&self) -> Result<Option<DeviceIdentity>, CoreError> {
            Ok(self.value.lock().expect("lock").clone())
        }

        fn save(&self, identity: &DeviceIdentity) -> Result<(), CoreError> {
            if self.refuse_save {
                return Err(CoreError::SecureStore("write refused".to_string()));
            }
            let stored = if self.corrupt_save {
                DeviceIdentity::generate("other-device", "Otro")
            } else {
                identity.clone()
            };
            *self.value.lock().expect("lock") = Some(stored);
            Ok(())
        }

        fn remove(&self) -> Result<(), CoreError> {
            *self.value.lock().expect("lock") = None;
            Ok(())
        }
    }

    fn legacy_identity() -> DeviceIdentity {
        DeviceIdentity::generate("device-old", "Escritorio")
    }

    #[test]
    fn only_the_legacy_identity_is_moved_and_then_forgotten() {
        let old = legacy_identity();
        let (current, legacy) = (Entry::default(), Entry::holding(&old));
        let store = MigratingStore {
            current: &current,
            legacy: &legacy,
        };
        let loaded = store.load().expect("load").expect("identity");
        assert_eq!(loaded.device_id(), "device-old");
        assert_eq!(current.now().as_deref(), Some("device-old"));
        assert_eq!(
            legacy.now(),
            None,
            "the legacy entry is removed after the check"
        );
    }

    #[test]
    fn a_current_identity_is_used_and_the_legacy_one_is_not_touched() {
        let (current, legacy) = (
            Entry::holding(&DeviceIdentity::generate("device-new", "Nuevo")),
            Entry::holding(&legacy_identity()),
        );
        let store = MigratingStore {
            current: &current,
            legacy: &legacy,
        };
        let loaded = store.load().expect("load").expect("identity");
        assert_eq!(loaded.device_id(), "device-new");
        assert_eq!(legacy.now().as_deref(), Some("device-old"));
    }

    #[test]
    fn a_refused_write_keeps_the_legacy_identity_and_uses_it() {
        let current = Entry {
            refuse_save: true,
            ..Entry::default()
        };
        let legacy = Entry::holding(&legacy_identity());
        let store = MigratingStore {
            current: &current,
            legacy: &legacy,
        };
        let loaded = store.load().expect("load").expect("identity");
        assert_eq!(loaded.device_id(), "device-old");
        assert_eq!(legacy.now().as_deref(), Some("device-old"));
        assert_eq!(current.now(), None);
    }

    #[test]
    fn a_read_back_that_differs_keeps_the_legacy_identity() {
        let current = Entry {
            corrupt_save: true,
            ..Entry::default()
        };
        let legacy = Entry::holding(&legacy_identity());
        let store = MigratingStore {
            current: &current,
            legacy: &legacy,
        };
        let loaded = store.load().expect("load").expect("identity");
        assert_eq!(loaded.device_id(), "device-old");
        assert_eq!(
            legacy.now().as_deref(),
            Some("device-old"),
            "never forgotten on a mismatch"
        );
        assert_eq!(current.now(), None, "the wrong copy is taken back");
    }

    #[test]
    fn with_no_identity_anywhere_there_is_none() {
        let (current, legacy) = (Entry::default(), Entry::default());
        let store = MigratingStore {
            current: &current,
            legacy: &legacy,
        };
        assert!(store.load().expect("load").is_none());
        assert_eq!(current.now(), None);
    }
}
