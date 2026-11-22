use std::fmt;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

use crate::{PakeError, Role};

const DOMAIN: &[u8] = b"excubia-pake-confirm-v1\0";

/// The 32-byte key both sides share when the code and the identities matched. It is never exposed:
/// its only use is to confirm, to each other, that they have it and that they are on the same TLS channel.
pub struct SessionKey([u8; 32]);

impl SessionKey {
    pub(crate) fn from_slice(bytes: &[u8]) -> Result<SessionKey, PakeError> {
        let key: [u8; 32] = bytes.try_into().map_err(|_| PakeError::Internal)?;
        Ok(SessionKey(key))
    }

    /// HMAC-SHA256(key, domain || role || TLS exporter || host SPKI hash), made by `role` for its peer.
    /// `tls_exporter` is the RFC 9266 `tls-exporter` value of the connection carrying this exchange; `spki` is the
    /// SHA-256 of the host certificate's SubjectPublicKeyInfo. A man in the middle with two TLS sessions has two
    /// different exporters, so the tag one side makes does not verify on the other.
    pub fn confirm_tag(
        &self,
        role: Role,
        tls_exporter: &[u8; 32],
        spki: &[u8; 32],
    ) -> Result<[u8; 32], PakeError> {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.0).map_err(|_| PakeError::Internal)?;
        mac.update(DOMAIN);
        mac.update(&[role.tag_byte()]);
        mac.update(tls_exporter);
        mac.update(spki);
        Ok(mac.finalize().into_bytes().into())
    }

    /// Checks, in constant time, the tag the peer (playing `peer_role`) sent.
    pub fn verify_confirm(
        &self,
        peer_role: Role,
        tls_exporter: &[u8; 32],
        spki: &[u8; 32],
        tag: &[u8],
    ) -> Result<(), PakeError> {
        let expected = self.confirm_tag(peer_role, tls_exporter, spki)?;
        // `ct_eq` on slices of different length is false without comparing contents.
        if bool::from(expected.as_slice().ct_eq(tag)) {
            Ok(())
        } else {
            Err(PakeError::ConfirmationFailed)
        }
    }

    /// For the vector tests only.
    #[cfg(feature = "test-vectors")]
    pub fn expose_for_tests(&self) -> [u8; 32] {
        self.0
    }
}

impl Drop for SessionKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for SessionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionKey(..)")
    }
}
