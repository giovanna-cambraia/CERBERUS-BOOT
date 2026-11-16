//! The vendor public key embedded in BL2.
//!
//! PLACEHOLDER: all zeros, so its SHA-256 never matches the OTP hash and BL2
//! halts. The crypto/provisioning step replaces this with the real key.
pub static VENDOR_PUBKEY: [u8; 32] = [0u8; 32];
