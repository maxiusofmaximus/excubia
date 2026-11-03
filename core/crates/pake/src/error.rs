use thiserror::Error;

/// Every way a pairing code or a PAKE exchange can fail. Variants never carry secret material.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PakeError {
    #[error("a pairing code is exactly 8 characters")]
    CodeLength,
    #[error("a pairing code only uses A-Z (without I and O) and 2-9")]
    CodeAlphabet,
    #[error("the host id and the device key must not be empty")]
    EmptyIdentity,
    #[error("the operating system could not provide random bytes")]
    Entropy,
    #[error("the PAKE message must be 33 bytes")]
    MessageLength,
    #[error("the PAKE message is our own, reflected back")]
    Reflected,
    #[error("the PAKE message is malformed or from the wrong side")]
    BadMessage,
    #[error("key confirmation failed")]
    ConfirmationFailed,
    #[error("unexpected output from the PAKE library")]
    Internal,
}
