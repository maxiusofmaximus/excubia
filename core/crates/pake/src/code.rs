use std::fmt;

use rand_core::{OsRng, RngCore};
use zeroize::Zeroize;

use crate::PakeError;

/// 32 symbols, so 5 random bits pick one without bias. No `I`, `O`, `0`, `1`.
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// Characters in a code: 8 x 5 bits = 40 bits.
pub const CODE_LEN: usize = 8;

/// The one-attempt pairing code: the PAKE password. Not `Clone`: every copy is one more place to wipe.
pub struct Code([u8; CODE_LEN]);

impl Code {
    /// A fresh code from the operating system's CSPRNG.
    pub fn generate() -> Result<Code, PakeError> {
        let mut random = [0u8; CODE_LEN];
        OsRng
            .try_fill_bytes(&mut random)
            .map_err(|_| PakeError::Entropy)?;
        let mut code = [0u8; CODE_LEN];
        for (symbol, byte) in code.iter_mut().zip(random.iter()) {
            *symbol = ALPHABET[usize::from(byte & 31)];
        }
        random.zeroize();
        Ok(Code(code))
    }

    /// Reads what a person typed: case and `-` are ignored, anything else must fit the alphabet.
    pub fn parse(text: &str) -> Result<Code, PakeError> {
        let mut code = [0u8; CODE_LEN];
        let mut count = 0;
        for byte in text.bytes().filter(|byte| *byte != b'-') {
            let byte = byte.to_ascii_uppercase();
            if count < CODE_LEN {
                code[count] = byte;
            }
            count += 1;
        }
        let result = if count != CODE_LEN {
            Err(PakeError::CodeLength)
        } else if code.iter().all(|byte| ALPHABET.contains(byte)) {
            return Ok(Code(code));
        } else {
            Err(PakeError::CodeAlphabet)
        };
        code.zeroize();
        result
    }

    /// The canonical text, to show it to the person. This is the secret: handle it as such.
    pub fn reveal(&self) -> &str {
        // The array only ever holds ASCII alphabet bytes, checked in `generate` and `parse`.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for Code {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Code(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cases() {
        let cases: [(&str, Result<&str, PakeError>); 9] = [
            ("ABCDEFGH", Ok("ABCDEFGH")),
            ("abcdefgh", Ok("ABCDEFGH")),
            ("abcd-efgh", Ok("ABCDEFGH")),
            ("A-B-C-D-E-F-G-H", Ok("ABCDEFGH")),
            ("ABCDEFG", Err(PakeError::CodeLength)),
            ("ABCDEFGHJ", Err(PakeError::CodeLength)),
            ("ABCDEFG0", Err(PakeError::CodeAlphabet)),
            ("ABCDEFGO", Err(PakeError::CodeAlphabet)),
            ("ABCDEFG\u{e9}", Err(PakeError::CodeLength)),
        ];
        for (text, expected) in cases {
            let got = Code::parse(text).map(|code| code.reveal().to_owned());
            assert_eq!(got, expected.map(str::to_owned), "{text}");
        }
        assert_eq!(Code::parse("ABCDEFG1").err(), Some(PakeError::CodeAlphabet));
        assert_eq!(Code::parse("").err(), Some(PakeError::CodeLength));
    }

    #[test]
    fn generated_codes_parse_back_and_differ() {
        let first = Code::generate().expect("rng");
        let second = Code::generate().expect("rng");
        assert_eq!(first.reveal().len(), CODE_LEN);
        assert_eq!(
            Code::parse(first.reveal()).expect("parse").reveal(),
            first.reveal()
        );
        assert_ne!(first.reveal(), second.reveal());
    }

    #[test]
    fn debug_does_not_leak() {
        let code = Code::parse("ABCDEFGH").expect("parse");
        assert!(!format!("{code:?}").contains("ABCD"));
    }
}
