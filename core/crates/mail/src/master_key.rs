//! Moving the mail master key to its new Credential Manager name without losing
//! it: the accounts cannot be read without it.

use hn_core_persist::credential::SecretStore;

/// The master key in `store`, moved there from `legacy` when only the legacy
/// name holds one. The legacy copy is forgotten only after the new name reads
/// back the same bytes; any other outcome keeps it and uses it.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn adopt_master_key(
    store: &dyn SecretStore,
    legacy: &dyn SecretStore,
) -> std::io::Result<Option<Vec<u8>>> {
    if let Some(key) = store.read()? {
        return Ok(Some(key));
    }
    let Some(old) = legacy.read()? else {
        return Ok(None);
    };
    if store.write(&old).is_err() {
        return Ok(Some(old));
    }
    if store.read().ok().flatten().as_deref() != Some(old.as_slice()) {
        let _ = store.delete();
        return Ok(Some(old));
    }
    let _ = legacy.delete();
    Ok(Some(old))
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::Mutex;

    use super::adopt_master_key;
    use hn_core_persist::credential::SecretStore;

    /// An in-memory secret whose write, read-back or delete can be made to fail.
    #[derive(Default)]
    struct Fake {
        value: Mutex<Option<Vec<u8>>>,
        fail_write: bool,
        corrupt_write: bool,
        fail_delete: bool,
    }

    impl Fake {
        fn holding(value: &[u8]) -> Self {
            Self {
                value: Mutex::new(Some(value.to_vec())),
                ..Self::default()
            }
        }

        fn empty() -> Self {
            Self::default()
        }

        fn now(&self) -> Option<Vec<u8>> {
            self.value.lock().expect("lock").clone()
        }
    }

    impl SecretStore for Fake {
        fn read(&self) -> io::Result<Option<Vec<u8>>> {
            Ok(self.now())
        }

        fn write(&self, value: &[u8]) -> io::Result<()> {
            if self.fail_write {
                return Err(io::Error::other("write refused"));
            }
            let stored = if self.corrupt_write {
                value.iter().map(|byte| byte ^ 1).collect()
            } else {
                value.to_vec()
            };
            *self.value.lock().expect("lock") = Some(stored);
            Ok(())
        }

        fn delete(&self) -> io::Result<()> {
            if self.fail_delete {
                return Err(io::Error::other("delete refused"));
            }
            *self.value.lock().expect("lock") = None;
            Ok(())
        }
    }

    const OLD: &[u8] = b"old-master-key-32-bytes-of-data!";

    #[test]
    fn only_the_legacy_key_is_moved_and_then_forgotten() {
        let (new, old) = (Fake::empty(), Fake::holding(OLD));
        let key = adopt_master_key(&new, &old).expect("adopt");
        assert_eq!(key.as_deref(), Some(OLD));
        assert_eq!(new.now().as_deref(), Some(OLD), "the new name holds it");
        assert_eq!(
            old.now(),
            None,
            "the legacy copy is removed after the check"
        );
    }

    #[test]
    fn a_new_key_is_read_and_the_legacy_one_is_not_touched() {
        let (new, old) = (Fake::holding(b"new"), Fake::holding(OLD));
        let key = adopt_master_key(&new, &old).expect("adopt");
        assert_eq!(key.as_deref(), Some(&b"new"[..]));
        assert_eq!(old.now().as_deref(), Some(OLD));
    }

    #[test]
    fn with_no_key_anywhere_there_is_none_to_move() {
        let (new, old) = (Fake::empty(), Fake::empty());
        assert_eq!(adopt_master_key(&new, &old).expect("adopt"), None);
        assert_eq!(new.now(), None);
    }

    #[test]
    fn a_failed_write_keeps_the_legacy_key_and_uses_it() {
        let new = Fake {
            fail_write: true,
            ..Fake::empty()
        };
        let old = Fake::holding(OLD);
        let key = adopt_master_key(&new, &old).expect("adopt");
        assert_eq!(key.as_deref(), Some(OLD));
        assert_eq!(old.now().as_deref(), Some(OLD));
        assert_eq!(new.now(), None);
    }

    #[test]
    fn a_read_back_that_differs_keeps_the_legacy_key() {
        let new = Fake {
            corrupt_write: true,
            ..Fake::empty()
        };
        let old = Fake::holding(OLD);
        let key = adopt_master_key(&new, &old).expect("adopt");
        assert_eq!(key.as_deref(), Some(OLD));
        assert_eq!(
            old.now().as_deref(),
            Some(OLD),
            "never forgotten on a mismatch"
        );
        assert_eq!(new.now(), None, "the wrong copy is taken back");
    }

    #[test]
    fn a_failed_delete_keeps_both_copies_and_still_uses_the_key() {
        let new = Fake::empty();
        let old = Fake {
            fail_delete: true,
            ..Fake::holding(OLD)
        };
        let key = adopt_master_key(&new, &old).expect("adopt");
        assert_eq!(key.as_deref(), Some(OLD));
        assert_eq!(new.now().as_deref(), Some(OLD));
        assert_eq!(old.now().as_deref(), Some(OLD));
    }
}
