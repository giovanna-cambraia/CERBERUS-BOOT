//! Verification errors. `code()` matches the `expect` names in
//! test/vectors/manifest.toml and `step()` matches its `step` numbers
//! (position in the verifier check order; 0 = length/framing)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    TooShort,
    SizeMismatch,
    BadMagic,
    UnsupportedVersion,
    CrcMismatch,
    ReservedNotZero,
    UnknownImageType,
    WrongImageType,
    BadPayloadSize,
    BadLoadAddress,
    RollbackRejected,
    DigestMismatch,
    KeyHashMismatch,
    BadSignature,
}

impl Error {
    pub const fn code(self) -> &'static str {
        match self {
            Error::TooShort => "TooShort",
            Error::SizeMismatch => "SizeMismatch",
            Error::BadMagic => "BadMagic",
            Error::UnsupportedVersion => "UnsupportedVersion",
            Error::CrcMismatch => "CrcMismatch",
            Error::ReservedNotZero => "ReservedNotZero",
            Error::UnknownImageType => "UnknownImageType",
            Error::WrongImageType => "WrongImageType",
            Error::BadPayloadSize => "BadPayloadSize",
            Error::BadLoadAddress => "BadLoadAddress",
            Error::RollbackRejected => "RollbackRejected",
            Error::DigestMismatch => "DigestMismatch",
            Error::KeyHashMismatch => "KeyHashMismatch",
            Error::BadSignature => "BadSignature",
        }
    }

    pub const fn step(self) -> u8 {
        match self {
            Error::TooShort | Error::SizeMismatch => 0,
            Error::BadMagic => 1,
            Error::UnsupportedVersion => 2,
            Error::CrcMismatch => 3,
            Error::ReservedNotZero => 4,
            Error::UnknownImageType | Error::WrongImageType => 5,
            Error::BadPayloadSize => 6,
            Error::BadLoadAddress => 7,
            Error::RollbackRejected => 8,
            Error::DigestMismatch => 9,
            Error::KeyHashMismatch | Error::BadSignature => 10,
        }
    }
}
