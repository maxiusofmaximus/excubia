//! The one short-lived PAKE pairing code (PAIRING-F2B2, `docs/contracts/pairing.md` F2a).
//!
//! The code is the PAKE password, so the host needs it in the clear for as long as it lives. It therefore lives
//! here, in memory only: never in `devices.json`, never in the `codes` store that `POST /v1/pair` reads, and never in
//! a log. One slot, one code, one use: `take` empties it, and `arm` wipes whatever was there before.
//!
//! After `start` the slot also holds the one pending session (F2b3): the key both sides derived, waiting for the
//! device's confirmation tag. It lives 60 s, is used once, and a new `arm` drops it.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use excubia_core_pake::{Code, PakeError, SessionKey};

/// How long a code lives, and the longest `arm` accepts (a longer `ttl` is cut to this).
pub const PAKE_CODE_TTL: Duration = Duration::from_secs(180);

/// How long a started ceremony waits for the device's confirmation.
pub const PAKE_PENDING_TTL: Duration = Duration::from_secs(60);

/// The code as it is shown to the person, once. It is a [`Code`]: its `Debug` is redacted and its drop wipes it.
pub type ShownCode = Code;

type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

struct Entry {
    code: Code,
    host_id: String,
    expires: Instant,
}

/// A started ceremony. `SessionKey` wipes itself when dropped; the public key is not a secret.
pub struct PendingPake {
    pub key: SessionKey,
    pub device_pubkey: Vec<u8>,
}

struct PendingEntry {
    session: PendingPake,
    expires: Instant,
}

/// Holds at most one armed code. `Code` wipes itself when dropped, so replacing, taking, expiring or clearing
/// the entry all end with the old secret overwritten.
pub struct PakeSlot {
    entry: Mutex<Option<Entry>>,
    pending: Mutex<Option<PendingEntry>>,
    clock: Clock,
}

impl PakeSlot {
    pub fn new() -> Self {
        Self::with_clock(Arc::new(Instant::now))
    }

    /// A slot that reads time from `clock`, so a test can expire a code without sleeping.
    pub fn with_clock(clock: Clock) -> Self {
        Self {
            entry: Mutex::new(None),
            pending: Mutex::new(None),
            clock,
        }
    }

    /// Generates a code for `host_id`, replacing any earlier one, and returns the copy to show. `ttl` is cut to
    /// [`PAKE_CODE_TTL`]. The slot keeps its own copy: the returned one is the caller's to show and drop.
    pub fn arm(&self, ttl: Duration, host_id: &str) -> Result<ShownCode, PakeError> {
        let code = Code::generate()?;
        let shown = Code::parse(code.reveal())?;
        let entry = Entry {
            code,
            host_id: host_id.to_owned(),
            expires: (self.clock)() + ttl.min(PAKE_CODE_TTL),
        };
        // The old entry, if any, is dropped (and so wiped) by this assignment. So is a session of the old code.
        *self.lock() = Some(entry);
        *self.lock_pending() = None;
        Ok(shown)
    }

    /// Consumes the slot: `Some` once per `arm`, and `None` if it expired or was already taken. An expired code
    /// is wiped, not returned.
    pub fn take(&self) -> Option<(Code, String)> {
        let entry = self.lock().take()?;
        ((self.clock)() < entry.expires).then_some((entry.code, entry.host_id))
    }

    /// Keeps the session of a `start` for [`PAKE_PENDING_TTL`], replacing (and wiping) any earlier one.
    pub fn hold_pending(&self, key: SessionKey, device_pubkey: Vec<u8>) {
        *self.lock_pending() = Some(PendingEntry {
            session: PendingPake { key, device_pubkey },
            expires: (self.clock)() + PAKE_PENDING_TTL,
        });
    }

    /// Consumes the pending session: `Some` once per `hold_pending`, `None` if it expired or was already taken.
    pub fn take_pending(&self) -> Option<PendingPake> {
        let entry = self.lock_pending().take()?;
        ((self.clock)() < entry.expires).then_some(entry.session)
    }

    /// Whether a ceremony can still start or finish: an armed code that has not expired, or a pending session
    /// that has not expired. Read-only; the host announces itself on the LAN only while this is true.
    pub fn is_live(&self) -> bool {
        let now = (self.clock)();
        self.lock().as_ref().is_some_and(|e| now < e.expires)
            || self
                .lock_pending()
                .as_ref()
                .is_some_and(|p| now < p.expires)
    }

    /// Empties the slot, the code and the pending session both.
    pub fn clear(&self) {
        *self.lock() = None;
        *self.lock_pending() = None;
    }

    fn lock(&self) -> MutexGuard<'_, Option<Entry>> {
        // The guarded value is a plain `Option`: a panic elsewhere cannot leave it half-written.
        self.entry.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_pending(&self) -> MutexGuard<'_, Option<PendingEntry>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for PakeSlot {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for PakeSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PakeSlot(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A slot and a handle that moves its clock forward.
    fn slot_with_clock() -> (PakeSlot, Arc<Mutex<Duration>>) {
        let base = Instant::now();
        let offset = Arc::new(Mutex::new(Duration::ZERO));
        let seen = Arc::clone(&offset);
        let clock: Clock = Arc::new(move || base + *seen.lock().expect("clock"));
        (PakeSlot::with_clock(clock), offset)
    }

    #[test]
    fn the_code_is_used_once() {
        let slot = PakeSlot::new();
        let shown = slot.arm(PAKE_CODE_TTL, "host-a").expect("arm");
        let (code, host_id) = slot.take().expect("first take");
        assert_eq!(code.reveal(), shown.reveal());
        assert_eq!(host_id, "host-a");
        assert!(slot.take().is_none(), "the second take must find nothing");
    }

    #[test]
    fn an_expired_code_is_gone() {
        let (slot, offset) = slot_with_clock();
        slot.arm(PAKE_CODE_TTL, "host").expect("arm");
        *offset.lock().expect("clock") = PAKE_CODE_TTL - Duration::from_secs(1);
        let (code, _) = slot.take().expect("still alive just before the TTL");
        drop(code);

        slot.arm(PAKE_CODE_TTL, "host").expect("arm");
        *offset.lock().expect("clock") = PAKE_CODE_TTL * 2;
        assert!(slot.take().is_none(), "expired");
        assert!(slot.take().is_none(), "and it stays empty");
    }

    #[test]
    fn a_ttl_longer_than_the_maximum_is_cut() {
        let (slot, offset) = slot_with_clock();
        slot.arm(Duration::MAX, "host")
            .expect("arm must not overflow");
        *offset.lock().expect("clock") = PAKE_CODE_TTL + Duration::from_secs(1);
        assert!(slot.take().is_none());
    }

    #[test]
    fn arming_again_replaces_the_previous_code() {
        let slot = PakeSlot::new();
        let first = slot.arm(PAKE_CODE_TTL, "host-1").expect("arm");
        let second = slot.arm(PAKE_CODE_TTL, "host-2").expect("arm");
        let (code, host_id) = slot.take().expect("take");
        assert_eq!(code.reveal(), second.reveal());
        assert_eq!(host_id, "host-2");
        assert_ne!(code.reveal(), first.reveal());
        assert!(slot.take().is_none(), "the first code did not survive");
    }

    #[test]
    fn clear_empties_the_slot() {
        let slot = PakeSlot::new();
        slot.arm(PAKE_CODE_TTL, "host").expect("arm");
        slot.clear();
        assert!(slot.take().is_none());
    }

    /// A real key, from two runs over the same code (the only way to get one).
    fn a_key(code: &Code, device: &[u8]) -> SessionKey {
        use excubia_core_pake::{Role, Session};
        let (host, host_message) = Session::start(Role::Host, code, "host", device).expect("host");
        let (device_side, device_message) =
            Session::start(Role::Device, code, "host", device).expect("device");
        drop(device_side.finish(&host_message));
        host.finish(&device_message).expect("key")
    }

    fn tag_of(key: &SessionKey) -> [u8; 32] {
        key.confirm_tag(excubia_core_pake::Role::Host, &[1; 32], &[2; 32])
            .expect("tag")
    }

    #[test]
    fn the_pending_session_is_used_once_and_lives_sixty_seconds() {
        let (slot, offset) = slot_with_clock();
        let code = Code::generate().expect("rng");
        slot.hold_pending(a_key(&code, b"dev-1"), b"dev-1".to_vec());
        let pending = slot.take_pending().expect("first take");
        assert_eq!(pending.device_pubkey, b"dev-1");
        assert!(slot.take_pending().is_none(), "one use");

        slot.hold_pending(a_key(&code, b"dev-1"), b"dev-1".to_vec());
        *offset.lock().expect("clock") = PAKE_PENDING_TTL - Duration::from_secs(1);
        assert!(slot.take_pending().is_some(), "alive just before the TTL");
        slot.hold_pending(a_key(&code, b"dev-1"), b"dev-1".to_vec());
        *offset.lock().expect("clock") = PAKE_PENDING_TTL * 2;
        assert!(slot.take_pending().is_none(), "expired");
    }

    #[test]
    fn holding_again_replaces_the_pending_session() {
        let slot = PakeSlot::new();
        let code = Code::generate().expect("rng");
        let (first, second) = (a_key(&code, b"dev-1"), a_key(&code, b"dev-2"));
        let second_tag = tag_of(&second);
        assert_ne!(tag_of(&first), second_tag);
        slot.hold_pending(first, b"dev-1".to_vec());
        slot.hold_pending(second, b"dev-2".to_vec());
        let pending = slot.take_pending().expect("take");
        assert_eq!(pending.device_pubkey, b"dev-2");
        assert_eq!(tag_of(&pending.key), second_tag);
        assert!(slot.take_pending().is_none());
    }

    #[test]
    fn arming_or_clearing_drops_the_pending_session() {
        let slot = PakeSlot::new();
        let code = Code::generate().expect("rng");
        slot.hold_pending(a_key(&code, b"dev"), b"dev".to_vec());
        slot.arm(PAKE_CODE_TTL, "host").expect("arm");
        assert!(
            slot.take_pending().is_none(),
            "a new code ends the old ceremony"
        );

        slot.hold_pending(a_key(&code, b"dev"), b"dev".to_vec());
        slot.clear();
        assert!(slot.take_pending().is_none());
    }

    #[test]
    fn a_code_is_live_until_it_expires_is_taken_or_cleared() {
        let (slot, offset) = slot_with_clock();
        assert!(!slot.is_live(), "an empty slot is not live");

        slot.arm(PAKE_CODE_TTL, "host").expect("arm");
        assert!(slot.is_live());
        assert!(slot.is_live(), "asking does not consume anything");
        *offset.lock().expect("clock") = PAKE_CODE_TTL - Duration::from_secs(1);
        assert!(slot.is_live(), "alive just before the TTL");
        *offset.lock().expect("clock") = PAKE_CODE_TTL;
        assert!(!slot.is_live(), "expired");

        *offset.lock().expect("clock") = Duration::ZERO;
        slot.arm(PAKE_CODE_TTL, "host").expect("arm");
        assert!(slot.is_live());
        drop(slot.take());
        assert!(!slot.is_live(), "a taken code is not live");

        slot.arm(PAKE_CODE_TTL, "host").expect("arm");
        slot.clear();
        assert!(!slot.is_live(), "a cleared slot is not live");
    }

    #[test]
    fn a_pending_session_keeps_the_slot_live_until_it_expires_or_is_taken() {
        let (slot, offset) = slot_with_clock();
        let code = Code::generate().expect("rng");
        slot.hold_pending(a_key(&code, b"dev"), b"dev".to_vec());
        assert!(slot.is_live());
        *offset.lock().expect("clock") = PAKE_PENDING_TTL - Duration::from_secs(1);
        assert!(slot.is_live());
        *offset.lock().expect("clock") = PAKE_PENDING_TTL;
        assert!(!slot.is_live(), "expired");

        *offset.lock().expect("clock") = Duration::ZERO;
        slot.arm(PAKE_CODE_TTL, "host").expect("arm");
        drop(slot.take());
        slot.hold_pending(a_key(&code, b"dev"), b"dev".to_vec());
        assert!(
            slot.is_live(),
            "the start burned the code, the session lives"
        );
        drop(slot.take_pending());
        assert!(!slot.is_live(), "a finished ceremony is not live");
    }

    #[test]
    fn debug_does_not_show_the_code() {
        let slot = PakeSlot::new();
        let shown = slot.arm(PAKE_CODE_TTL, "host").expect("arm");
        let text = shown.reveal().to_owned();
        for debug in [format!("{shown:?}"), format!("{slot:?}")] {
            assert!(!debug.contains(&text), "{debug}");
        }
        let (code, _) = slot.take().expect("take");
        assert!(!format!("{code:?}").contains(&text));
    }
}
