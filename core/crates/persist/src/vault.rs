//! The vault key of the chat database (`docs/rfc/vault-chatdb-encryption.md`).
//!
//! The key is 256 bits: HKDF-SHA-512 over the OS random source and a pool of
//! pointer movements, kept in a named secret (the Credential Manager on Windows)
//! and written down once as a 24-word BIP-39 phrase. The OS random source always
//! takes part: the movements can add entropy but never subtract any, so an empty
//! pool gives the same key strength as "no mouse".

use std::fmt;
use std::io;

use bip39::{Language, Mnemonic};
use hkdf::Hkdf;
use sha2::{Digest, Sha512};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

#[cfg(windows)]
use crate::credential::CredentialManager;
use crate::credential::SecretStore;
use crate::{fill_random, RandomError};

/// Samples the pool needs before the UI may say "enough": about ten seconds of movement.
pub const SAMPLES_NEEDED: usize = 2_048;
const KEY_LEN: usize = 32;
const PHRASE_WORDS: usize = 24;
const KEY_INFO: &[u8] = b"excubia/chat-db/v1";
const POOL_DOMAIN: &[u8] = b"excubia/entropy-pool/v1";
#[cfg(windows)]
const CREDENTIAL: &str = "excubia/chat-db-key";

/// The movement pool: a hash chain, `state = SHA-512(state ‖ sample)`. Each sample
/// adds at most about one bit; a sample equal to the previous one (a resting
/// pointer) adds nothing.
pub struct EntropyPool {
    state: [u8; 64],
    samples: usize,
    last: Option<(i32, i32)>,
}

impl EntropyPool {
    pub fn new() -> Self {
        let mut state = [0u8; 64];
        state.copy_from_slice(&Sha512::digest(POOL_DOMAIN));
        Self {
            state,
            samples: 0,
            last: None,
        }
    }

    pub fn add_sample(&mut self, x: i32, y: i32, micros: u64) {
        if self.last == Some((x, y)) {
            return;
        }
        self.last = Some((x, y));
        self.absorb(&[&x.to_le_bytes(), &y.to_le_bytes(), &micros.to_le_bytes()]);
        self.samples = self.samples.saturating_add(1);
    }

    /// Mixes raw bytes into the pool. They are not counted as pointer samples.
    pub fn add_bytes(&mut self, bytes: &[u8]) {
        self.absorb(&[bytes]);
    }

    pub fn samples(&self) -> usize {
        self.samples
    }

    pub fn is_enough(&self) -> bool {
        self.samples >= SAMPLES_NEEDED
    }

    pub fn digest(&self) -> [u8; 64] {
        self.state
    }

    fn absorb(&mut self, parts: &[&[u8]]) {
        let mut hasher = Sha512::new();
        hasher.update(self.state);
        for part in parts {
            hasher.update(part);
        }
        self.state.copy_from_slice(&hasher.finalize());
    }
}

impl Default for EntropyPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for EntropyPool {
    fn drop(&mut self) {
        self.state.zeroize();
    }
}

/// The 256-bit key of the chat database. It is erased when dropped, and `Debug`
/// prints no part of it.
pub struct VaultKey([u8; KEY_LEN]);

/// A recovery phrase that is not a 24-word BIP-39 phrase with a valid checksum.
#[derive(Debug, Error)]
pub enum PhraseError {
    #[error("the recovery phrase must have 24 words")]
    WordCount,
    #[error("the recovery phrase is not a valid BIP-39 phrase")]
    Invalid,
}

impl VaultKey {
    /// HKDF-SHA-512 over 64 bytes of the OS random source and the pool digest,
    /// with a fresh 32-byte salt from the OS random source.
    pub fn generate(pool: &EntropyPool) -> Result<Self, RandomError> {
        let mut system = Zeroizing::new([0u8; 64]);
        fill_random(&mut system[..])?;
        let mut salt = [0u8; 32];
        fill_random(&mut salt)?;
        let mut ikm = Zeroizing::new(Vec::with_capacity(128));
        ikm.extend_from_slice(&system[..]);
        ikm.extend_from_slice(&pool.digest());
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        Hkdf::<Sha512>::new(Some(&salt), &ikm)
            // expand only fails above 255 × 64 bytes; 32 never reaches that.
            .expand(KEY_INFO, &mut key[..])
            .map_err(|_| RandomError)?;
        Ok(Self(*key))
    }

    /// The `PRAGMA key` value for a raw key: `x'<64 hex digits>'`. SQLCipher then
    /// skips its passphrase derivation, which the random key does not need.
    pub fn sqlcipher_pragma(&self) -> Zeroizing<String> {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = Zeroizing::new(String::with_capacity(3 + 2 * KEY_LEN));
        text.push_str("x'");
        for byte in self.0 {
            text.push(char::from(HEX[usize::from(byte >> 4)]));
            text.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        text.push('\'');
        text
    }

    /// The key as 24 English BIP-39 words, for the user to write down.
    pub fn to_phrase(&self) -> Result<Zeroizing<String>, PhraseError> {
        let mnemonic = Mnemonic::from_entropy(&self.0).map_err(|_| PhraseError::Invalid)?;
        Ok(Zeroizing::new(mnemonic.to_string()))
    }

    /// Reads a phrase back. Spacing and capitals do not matter; the word count and
    /// the checksum must.
    pub fn from_phrase(phrase: &str) -> Result<Self, PhraseError> {
        let words: Vec<String> = phrase
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .collect();
        if words.len() != PHRASE_WORDS {
            return Err(PhraseError::WordCount);
        }
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, &words.join(" "))
            .map_err(|_| PhraseError::Invalid)?;
        let (entropy, length) = mnemonic.to_entropy_array();
        let bytes: [u8; KEY_LEN] = entropy[..length]
            .try_into()
            .map_err(|_| PhraseError::Invalid)?;
        Ok(Self(bytes))
    }
}

impl fmt::Debug for VaultKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VaultKey(..)")
    }
}

impl Drop for VaultKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// The vault key kept in one named secret. A blob that is not 32 bytes is refused.
pub struct VaultKeyStore<S: SecretStore> {
    secret: S,
}

impl<S: SecretStore> VaultKeyStore<S> {
    pub fn new(secret: S) -> Self {
        Self { secret }
    }

    pub fn load(&self) -> io::Result<Option<VaultKey>> {
        let Some(blob) = self.secret.read()? else {
            return Ok(None);
        };
        let blob = Zeroizing::new(blob);
        let bytes: [u8; KEY_LEN] = blob
            .as_slice()
            .try_into()
            .map_err(|_| io::Error::other("the vault key has the wrong length"))?;
        Ok(Some(VaultKey(bytes)))
    }

    pub fn save(&self, key: &VaultKey) -> io::Result<()> {
        self.secret.write(&key.0)
    }

    pub fn delete(&self) -> io::Result<()> {
        self.secret.delete()
    }
}

#[cfg(windows)]
impl VaultKeyStore<CredentialManager> {
    pub fn credential_manager() -> Self {
        Self::new(CredentialManager::new(CREDENTIAL))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A secret kept in memory, so the store's rules are tested without the OS.
    #[derive(Default)]
    struct MemorySecret(Mutex<Option<Vec<u8>>>);

    impl SecretStore for MemorySecret {
        fn read(&self) -> io::Result<Option<Vec<u8>>> {
            Ok(self.0.lock().expect("lock").clone())
        }
        fn write(&self, value: &[u8]) -> io::Result<()> {
            *self.0.lock().expect("lock") = Some(value.to_vec());
            Ok(())
        }
        fn delete(&self) -> io::Result<()> {
            *self.0.lock().expect("lock") = None;
            Ok(())
        }
    }

    /// The all-zero key: its phrase is the well-known "abandon … art".
    fn zero_key() -> VaultKey {
        VaultKey([0u8; KEY_LEN])
    }

    #[test]
    fn two_keys_from_the_same_pool_differ() {
        let pool = EntropyPool::new();
        let first = VaultKey::generate(&pool).expect("first");
        let second = VaultKey::generate(&pool).expect("second");
        assert_ne!(*first.sqlcipher_pragma(), *second.sqlcipher_pragma());
        let pragma = first.sqlcipher_pragma();
        assert!(pragma.starts_with("x'") && pragma.ends_with('\'') && pragma.len() == 67);
    }

    #[test]
    fn a_repeated_sample_adds_nothing() {
        let mut pool = EntropyPool::new();
        pool.add_sample(10, 20, 1);
        let once = pool.digest();
        pool.add_sample(10, 20, 2);
        pool.add_sample(10, 20, 3);
        assert_eq!(pool.samples(), 1);
        assert_eq!(pool.digest(), once);
        pool.add_sample(11, 20, 4);
        assert_eq!(pool.samples(), 2);
    }

    #[test]
    fn enough_means_2048_samples() {
        let mut pool = EntropyPool::new();
        for i in 0..2_047 {
            pool.add_sample(i, 0, 0);
        }
        assert!(!pool.is_enough());
        pool.add_sample(2_047, 0, 0);
        assert!(pool.is_enough());
    }

    #[test]
    fn an_empty_pool_still_makes_a_key() {
        let key = VaultKey::generate(&EntropyPool::new()).expect("key");
        assert!(key.sqlcipher_pragma().starts_with("x'"));
    }

    #[test]
    fn the_phrase_is_the_known_abandon_art_and_reads_back() {
        let phrase = zero_key().to_phrase().expect("phrase");
        assert!(phrase.starts_with("abandon abandon abandon"));
        assert!(phrase.ends_with(" art"));
        let back = VaultKey::from_phrase(&format!("  ABANDON {}  ", &phrase[8..])).expect("back");
        assert_eq!(*back.sqlcipher_pragma(), *zero_key().sqlcipher_pragma());
    }

    #[test]
    fn a_phrase_with_one_word_changed_is_refused() {
        // "abandon" has index 0; the checksum of the zero key is not 0.
        let changed = format!("{} abandon", "abandon ".repeat(23).trim_end());
        assert!(matches!(
            VaultKey::from_phrase(&changed),
            Err(PhraseError::Invalid)
        ));
    }

    #[test]
    fn a_phrase_of_another_length_is_refused() {
        let short = "abandon ".repeat(11) + "about";
        assert!(matches!(
            VaultKey::from_phrase(&short),
            Err(PhraseError::WordCount)
        ));
    }

    #[test]
    fn debug_shows_no_key_material() {
        assert_eq!(format!("{:?}", zero_key()), "VaultKey(..)");
    }

    #[test]
    fn the_store_round_trips_and_forgets() {
        let store = VaultKeyStore::new(MemorySecret::default());
        assert!(store.load().expect("empty").is_none());
        store.save(&zero_key()).expect("save");
        let loaded = store.load().expect("load").expect("some");
        assert_eq!(*loaded.sqlcipher_pragma(), *zero_key().sqlcipher_pragma());
        store.delete().expect("delete");
        assert!(store.load().expect("gone").is_none());
    }

    #[test]
    fn the_store_refuses_a_blob_of_31_bytes() {
        let store = VaultKeyStore::new(MemorySecret::default());
        store.secret.write(&[7u8; 31]).expect("write");
        assert!(store.load().is_err());
    }
}
