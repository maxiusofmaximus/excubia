//! Enrollment and recovery codes, per `docs/contracts/pairing.md`.
//!
//! Only hashes are ever stored; the plaintext exists in the minting process
//! and on the operator's screen. The canonical form drops separators and case,
//! so a code retyped from a screen still matches.

use data_encoding::BASE32;
use excubia_persist::{fill_random, RandomError};
use sha2::{Digest, Sha256};

/// 32 bits of entropy for a single-use code that lives three minutes.
pub const ENROLLMENT_CODE_BYTES: usize = 4;
/// 80 bits for a recovery code, printed once as four base32 groups.
pub const RECOVERY_CODE_BYTES: usize = 10;
/// Default life of a pairing code (`excubia-host pair-code --ttl` overrides it).
pub const ENROLLMENT_TTL_SECONDS: f64 = 180.0;

/// A new pairing code: base32, no padding (`AEBAGBA`).
pub fn mint_enrollment_code() -> Result<String, RandomError> {
    let mut bytes = [0u8; ENROLLMENT_CODE_BYTES];
    fill_random(&mut bytes)?;
    Ok(BASE32.encode(&bytes).trim_end_matches('=').to_string())
}

/// A new recovery code in its display form: four base32 groups of four.
pub fn mint_recovery_code() -> Result<String, RandomError> {
    let mut bytes = [0u8; RECOVERY_CODE_BYTES];
    fill_random(&mut bytes)?;
    let raw = BASE32.encode(&bytes).trim_end_matches('=').to_string();
    Ok(raw
        .as_bytes()
        .chunks(4)
        .map(|chunk| String::from_utf8_lossy(chunk).to_string())
        .collect::<Vec<_>>()
        .join("-"))
}

/// Drop separators and case, so any readable spelling hashes the same.
pub fn canonical_code(code: &str) -> String {
    code.chars()
        .filter(|character| !character.is_whitespace() && *character != '-')
        .flat_map(char::to_uppercase)
        .collect()
}

/// SHA-256 hex of the canonical form: what the trust store keeps.
pub fn code_hash(code: &str) -> String {
    let digest = Sha256::digest(canonical_code(code).as_bytes());
    hex::encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrollment_codes_are_seven_base32_characters() {
        let code = mint_enrollment_code().expect("random");
        // 4 bytes are 7 base32 characters, no padding.
        assert_eq!(code.len(), 7);
        assert!(code
            .chars()
            .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c)));
    }

    #[test]
    fn recovery_codes_are_four_groups_of_four() {
        let code = mint_recovery_code().expect("random");
        let groups: Vec<&str> = code.split('-').collect();
        assert_eq!(groups.len(), 4);
        assert!(groups.iter().all(|group| group.len() == 4));
    }

    // Pinned to Python's base64.b32encode for the same bytes, so the format is
    // the standard one and not merely self-consistent.
    #[test]
    fn base32_matches_the_standard_encoding() {
        let encoded = BASE32.encode(&[0x01, 0x02, 0x03, 0x04]);
        assert_eq!(encoded.trim_end_matches('='), "AEBAGBA");
        let encoded = BASE32.encode(&[0u8; 10]);
        assert_eq!(encoded, "AAAAAAAAAAAAAAAA");
    }

    #[test]
    fn canonical_form_ignores_case_and_separators() {
        assert_eq!(canonical_code(" 7qk3-m2xp "), "7QK3M2XP");
        assert_eq!(code_hash("7qk3-m2xp"), code_hash("7QK3M2XP"));
        assert_ne!(code_hash("AAAA"), code_hash("AAAB"));
    }
}
