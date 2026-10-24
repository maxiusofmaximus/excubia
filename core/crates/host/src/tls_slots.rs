//! The host's TLS identities under `<data-dir>/tls`, in three slots:
//!
//! - `active`: the certificate and key the LAN listener serves;
//! - `next`: a prepared replacement, present only between `prepare` and
//!   `activate`;
//! - `previous`: the certificate that was active before the last `activate`,
//!   kept as a certificate only (its key is not needed to pin it).
//!
//! Devices accept the active pin and the backup pin, which is the next one
//! while it is prepared and the previous one otherwise. That is what lets a
//! device keep working through a rotation without pairing again.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use hn_core_persist::{write_atomic, write_secret_atomic};
use hn_core_pin::{CertificateInfo, Fingerprint, Identity, PinSet};

use crate::paths;

const ACTIVE: &str = "active";
const NEXT: &str = "next";
const PREVIOUS: &str = "previous.cert.der";

pub struct TlsSlots {
    dir: PathBuf,
}

impl TlsSlots {
    pub fn open(data_dir: &Path) -> Self {
        Self {
            dir: paths::tls_dir(data_dir),
        }
    }

    /// The identity the LAN listener serves. The first call creates it.
    pub fn active(&self) -> Result<Identity> {
        if let Some(identity) = self.load(ACTIVE)? {
            return Ok(identity);
        }
        let identity = Identity::generate()?;
        self.store(ACTIVE, &identity)?;
        Ok(identity)
    }

    /// The pins a pairing link publishes: the active key and its backup.
    pub fn pins(&self) -> Result<PinSet> {
        let primary = self.active()?.fingerprint()?;
        let backup = match self.load(NEXT)? {
            Some(next) => Some(next.fingerprint()?),
            None => self.previous_pin()?,
        };
        Ok(PinSet::new(primary, backup))
    }

    /// Prepare the replacement and return its pin. Preparing twice keeps the
    /// first one, because its pin may already be on a device.
    pub fn prepare(&self) -> Result<Fingerprint> {
        let next = match self.load(NEXT)? {
            Some(next) => next,
            None => {
                let next = Identity::generate()?;
                self.store(NEXT, &next)?;
                next
            }
        };
        Ok(next.fingerprint()?)
    }

    /// Make the prepared identity active. Returns the new active pin and the
    /// pin it replaced, which stays published as the backup.
    pub fn activate(&self) -> Result<(Fingerprint, Fingerprint)> {
        let next = self
            .load(NEXT)?
            .context("nothing is prepared: run `host tls prepare` first")?;
        let previous = self.active()?;
        write_atomic(&self.dir.join(PREVIOUS), previous.cert_der())?;
        self.store(ACTIVE, &next)?;
        remove_slot(&self.dir, NEXT)?;
        Ok((next.fingerprint()?, previous.fingerprint()?))
    }

    fn previous_pin(&self) -> Result<Option<Fingerprint>> {
        match read_optional(&self.dir.join(PREVIOUS))? {
            Some(der) => Ok(Some(CertificateInfo::read(&der)?.fingerprint())),
            None => Ok(None),
        }
    }

    /// A slot is both files or neither. One without the other is a crash
    /// mid-write, so it is reported instead of guessed at.
    fn load(&self, slot: &str) -> Result<Option<Identity>> {
        let cert = read_optional(&cert_path(&self.dir, slot))?;
        let key = read_optional(&key_path(&self.dir, slot))?;
        match (cert, key) {
            (Some(cert), Some(key)) => Ok(Some(Identity::from_parts(cert, key))),
            (None, None) => Ok(None),
            _ => bail!(
                "the {slot} TLS identity in {} is incomplete; remove the tls directory and restart",
                self.dir.display()
            ),
        }
    }

    fn store(&self, slot: &str, identity: &Identity) -> Result<()> {
        fs::create_dir_all(&self.dir).with_context(|| format!("create {}", self.dir.display()))?;
        write_atomic(&cert_path(&self.dir, slot), identity.cert_der())?;
        write_secret_atomic(&key_path(&self.dir, slot), identity.key_der())?;
        Ok(())
    }
}

fn cert_path(dir: &Path, slot: &str) -> PathBuf {
    dir.join(format!("{slot}.cert.der"))
}

fn key_path(dir: &Path, slot: &str) -> PathBuf {
    dir.join(format!("{slot}.key.der"))
}

fn remove_slot(dir: &Path, slot: &str) -> Result<()> {
    for path in [cert_path(dir, slot), key_path(dir, slot)] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).with_context(|| format!("remove {}", path.display())),
        }
    }
    Ok(())
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hn_core_test_dir::TestDir;

    #[test]
    fn a_rotation_publishes_both_pins_and_then_keeps_the_old_one_as_backup() {
        let dir = TestDir::new("tls-slots-rotation");
        let slots = TlsSlots::open(dir.path());
        let before = slots.pins().expect("pins");
        assert_eq!(before.backup(), None);

        let next = slots.prepare().expect("prepare");
        let during = slots.pins().expect("pins");
        assert_eq!(during.primary(), before.primary());
        assert_eq!(during.backup(), Some(next));
        assert_eq!(
            slots.prepare().expect("again"),
            next,
            "prepare is idempotent"
        );

        let (active, previous) = slots.activate().expect("activate");
        assert_eq!(active, next);
        assert_eq!(previous, before.primary());
        let after = slots.pins().expect("pins");
        assert_eq!(after.primary(), next);
        assert_eq!(after.backup(), Some(previous));
    }

    #[test]
    fn activating_without_a_prepared_identity_is_refused() {
        let dir = TestDir::new("tls-slots-no-next");
        let slots = TlsSlots::open(dir.path());
        assert!(slots.activate().is_err());
    }

    #[test]
    fn an_incomplete_slot_is_an_error_not_a_new_identity() {
        let dir = TestDir::new("tls-slots-incomplete");
        let slots = TlsSlots::open(dir.path());
        slots.active().expect("create");
        fs::remove_file(key_path(&paths::tls_dir(dir.path()), ACTIVE)).expect("remove key");
        assert!(slots.active().is_err());
    }
}
