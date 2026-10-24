use crate::Fingerprint;

/// The keys a device accepts for one host: the one in use and, during a
/// rotation, the one that replaces it. Accepting either is what lets a device
/// survive the switch without re-pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinSet {
    primary: Fingerprint,
    backup: Option<Fingerprint>,
}

impl PinSet {
    pub fn new(primary: Fingerprint, backup: Option<Fingerprint>) -> Self {
        Self { primary, backup }
    }

    pub fn primary(&self) -> Fingerprint {
        self.primary
    }

    pub fn backup(&self) -> Option<Fingerprint> {
        self.backup
    }

    pub fn contains(&self, fingerprint: &Fingerprint) -> bool {
        self.primary == *fingerprint || self.backup == Some(*fingerprint)
    }
}
