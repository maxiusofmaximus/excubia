use std::fmt;

use curve25519_dalek::edwards::CompressedEdwardsY;
use rand_core::{CryptoRng, OsRng, RngCore};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use zeroize::{Zeroize, Zeroizing};

use crate::{Code, PakeError, SessionKey};

/// Side byte + compressed Edwards point.
pub const MESSAGE_LEN: usize = 33;
const ENTROPY_LEN: usize = 64;

/// Who is speaking. The device plays SPAKE2 side A (`idA` = its public key), the host side B (`idB` = its id).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Host,
    Device,
}

impl Role {
    fn side_byte(self) -> u8 {
        match self {
            Role::Device => b'A',
            Role::Host => b'B',
        }
    }

    pub(crate) fn tag_byte(self) -> u8 {
        match self {
            Role::Host => 1,
            Role::Device => 2,
        }
    }

    fn other(self) -> Role {
        match self {
            Role::Host => Role::Device,
            Role::Device => Role::Host,
        }
    }
}

/// One half of one SPAKE2 run. `finish` consumes it: a code is spent by a single run.
pub struct Session {
    role: Role,
    spake: Spake2<Ed25519Group>,
    own_message: Vec<u8>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Session({:?})", self.role)
    }
}

/// The 64 random bytes `Scalar::random` reads, handed over as an RNG so a failing OS RNG is an error, not a panic.
struct Entropy {
    bytes: [u8; ENTROPY_LEN],
    used: usize,
}

impl RngCore for Entropy {
    fn next_u32(&mut self) -> u32 {
        let mut word = [0u8; 4];
        self.fill_bytes(&mut word);
        u32::from_le_bytes(word)
    }

    fn next_u64(&mut self) -> u64 {
        let mut word = [0u8; 8];
        self.fill_bytes(&mut word);
        u64::from_le_bytes(word)
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        let from_buffer = dest.len().min(ENTROPY_LEN - self.used);
        dest[..from_buffer].copy_from_slice(&self.bytes[self.used..self.used + from_buffer]);
        self.used += from_buffer;
        // Never reached today (the scalar takes exactly 64 bytes); if it ever is, the rest is still fresh randomness.
        OsRng.fill_bytes(&mut dest[from_buffer..]);
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

impl CryptoRng for Entropy {}

impl Drop for Entropy {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

impl Session {
    /// Starts one side. `host_id` and `device_pubkey` are the identities idB / idA: both sides must pass the same ones.
    /// Returns the message to send to the peer.
    pub fn start(
        role: Role,
        code: &Code,
        host_id: &str,
        device_pubkey: &[u8],
    ) -> Result<(Session, Vec<u8>), PakeError> {
        let mut bytes = [0u8; ENTROPY_LEN];
        OsRng
            .try_fill_bytes(&mut bytes)
            .map_err(|_| PakeError::Entropy)?;
        Self::start_with(role, code, host_id, device_pubkey, bytes)
    }

    /// Same as [`Session::start`] with caller-chosen entropy, to reproduce vectors. Reusing it breaks the protocol.
    #[cfg(feature = "test-vectors")]
    pub fn start_with_entropy(
        role: Role,
        code: &Code,
        host_id: &str,
        device_pubkey: &[u8],
        entropy: [u8; 64],
    ) -> Result<(Session, Vec<u8>), PakeError> {
        Self::start_with(role, code, host_id, device_pubkey, entropy)
    }

    fn start_with(
        role: Role,
        code: &Code,
        host_id: &str,
        device_pubkey: &[u8],
        bytes: [u8; ENTROPY_LEN],
    ) -> Result<(Session, Vec<u8>), PakeError> {
        let entropy = Entropy { bytes, used: 0 };
        if host_id.is_empty() || device_pubkey.is_empty() {
            return Err(PakeError::EmptyIdentity);
        }
        let password = Password::new(code.as_bytes());
        let id_a = Identity::new(device_pubkey);
        let id_b = Identity::new(host_id.as_bytes());
        let (spake, message) = match role {
            Role::Device => {
                Spake2::<Ed25519Group>::start_a_with_rng(&password, &id_a, &id_b, entropy)
            }
            Role::Host => {
                Spake2::<Ed25519Group>::start_b_with_rng(&password, &id_a, &id_b, entropy)
            }
        };
        let session = Session {
            role,
            spake,
            own_message: message.clone(),
        };
        Ok((session, message))
    }

    /// Mixes in the peer's message and returns the shared key. The key only matches if both sides had the same code and
    /// identities; that is proven later by the confirmation tags, not here.
    ///
    /// A session cannot be finished twice (the second use does not compile):
    ///
    /// ```compile_fail,E0382
    /// use excubia_core_pake::{Code, Role, Session};
    /// let code = Code::generate().unwrap();
    /// let (session, message) = Session::start(Role::Host, &code, "host", b"key").unwrap();
    /// let _ = session.finish(&message);
    /// let _ = session.finish(&message);
    /// ```
    pub fn finish(self, peer_message: &[u8]) -> Result<SessionKey, PakeError> {
        if peer_message.len() != MESSAGE_LEN {
            return Err(PakeError::MessageLength);
        }
        if peer_message == self.own_message.as_slice() {
            return Err(PakeError::Reflected);
        }
        if peer_message[0] != self.role.other().side_byte() {
            return Err(PakeError::BadMessage);
        }
        check_point(&peer_message[1..])?;
        let key = Zeroizing::new(
            self.spake
                .finish(peer_message)
                .map_err(|_| PakeError::BadMessage)?,
        );
        SessionKey::from_slice(&key)
    }
}

/// `spake2` 0.4.0 accepts any curve point; python-spake2 only accepts non-zero points of the prime-order subgroup.
/// A small-order or mixed-order point is rejected here so both implementations accept the same set.
fn check_point(bytes: &[u8]) -> Result<(), PakeError> {
    let compressed = CompressedEdwardsY::from_slice(bytes).map_err(|_| PakeError::BadMessage)?;
    match compressed.decompress() {
        Some(point) if !point.is_small_order() && point.is_torsion_free() => Ok(()),
        _ => Err(PakeError::BadMessage),
    }
}
