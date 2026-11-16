//! Image verification pipeline (verifier steps 1-10 of image_header.md)
//! Crypto is injected through `CryptoEngine`, so this crate has no
//! dependencies and the same logic runs on the host (tests) and on target.

use crate::error::Error;
use crate::header::{Header, HEADER_SIZE, SIGNED_LEN};

pub trait CryptoEngine {
    fn sha256(&self, data: &[u8]) -> [u8; 32];
    /// Ed25519 verification, strict. Returns false on any failure, including an invalid public key encoding
    fn ed25519_verify(&self, public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool;
}

/// half-open address range [start, end)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub start: u32,
    pub end: u32,
}

/// everything the verifier needs from its environment
pub struct Policy<'a> {
    /// IMAGE_TYPE expected for this slot/stage (step 5)
    pub expected_type: u16,
    /// regions the payload may be loaded into (step 7)
    pub allowed_load: &'a [Region],
    /// current hardware monotonic counter value (step 8)
    pub rollback_counter: u32,
    /// vendor public key embedded in BL2
    pub public_key: &'a [u8; 32],
    /// SHA-256 of the vendor public key, read from the OTP fuses
    pub otp_key_hash: &'a [u8; 32],
}

pub(crate) fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

/// `image` must be exactly header || payload
pub fn verify_image<C: CryptoEngine>(
    image: &[u8],
    policy: &Policy<'_>,
    crypto: &C,
) -> Result<Header, Error> {
    // steps 1-6: structural checks
    let h = Header::parse(image, policy.expected_type)?;

    // framing: the buffer must be exactly the size the header declares
    let total = HEADER_SIZE + h.payload_size as usize;
    if image.len() != total {
        return Err(Error::SizeMismatch);
    }

    // step 7: the payload fits a permitted region and the entry point lies in it
    let load = u64::from(h.load_address);
    let size = u64::from(h.payload_size);
    let entry = u64::from(h.entry_point);
    let in_region = policy
        .allowed_load
        .iter()
        .any(|r| load >= u64::from(r.start) && load + size <= u64::from(r.end));
    if !in_region || entry < load || entry >= load + size {
        return Err(Error::BadLoadAddress);
    }

    // step 8: anti-rollback
    if h.security_version < policy.rollback_counter {
        return Err(Error::RollbackRejected);
    }

    // step 9: payload digest
    let digest = crypto.sha256(&image[HEADER_SIZE..]);
    if !ct_eq(&digest, &h.image_digest) {
        return Err(Error::DigestMismatch);
    }

    // step 10: the embedded key must match the OTP hash, then the signature
    let key_hash = crypto.sha256(policy.public_key);
    if !ct_eq(&key_hash, policy.otp_key_hash) {
        return Err(Error::KeyHashMismatch);
    }
    if !crypto.ed25519_verify(policy.public_key, &image[..SIGNED_LEN], &h.signature) {
        return Err(Error::BadSignature);
    }
    Ok(h)
}

#[cfg(test)]
mod tests {
    use super::ct_eq;

    #[test]
    fn ct_eq_behaviour() {
        assert!(ct_eq(&[1, 2, 3], &[1, 2, 3]));
        assert!(!ct_eq(&[1, 2, 3], &[1, 2, 4]));
        assert!(!ct_eq(&[1, 2, 3], &[1, 2]));
        assert!(ct_eq(&[], &[]));
    }
}
