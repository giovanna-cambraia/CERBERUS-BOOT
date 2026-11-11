//! Build and verify signed images: header (256 bytes) || payload.

use crate::header::{Header, HeaderError, HEADER_SIZE, MAX_PAYLOAD};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fmt;

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub struct ImageParams {
    pub image_type: u16,
    pub security_version: u32,
    pub load_address: u32,
    pub entry_point: u32,
    pub nonce_salt: [u8; 16],
}

#[derive(Debug, PartialEq, Eq)]
pub enum BuildError {
    EmptyPayload,
    PayloadTooLarge(usize),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::EmptyPayload => write!(f, "payload is empty"),
            BuildError::PayloadTooLarge(n) => {
                write!(f, "payload is {n} bytes, maximum is {MAX_PAYLOAD}")
            }
        }
    }
}

impl std::error::Error for BuildError {}

/// build `header || payload`; the digest covers the payload and the Ed25519 signature covers header bytes [0x000..0x047]
pub fn build_image(
    p: &ImageParams,
    payload: &[u8],
    key: &SigningKey,
) -> Result<Vec<u8>, BuildError> {
    if payload.is_empty() {
        return Err(BuildError::EmptyPayload);
    }
    if payload.len() > MAX_PAYLOAD as usize {
        return Err(BuildError::PayloadTooLarge(payload.len()));
    }

    let mut h = Header {
        image_type: p.image_type,
        security_version: p.security_version,
        payload_size: payload.len() as u32,
        load_address: p.load_address,
        entry_point: p.entry_point,
        nonce_salt: p.nonce_salt,
        image_digest: sha256(payload),
        signature: [0u8; 64],
    };
    // the signed range excludes the signature field, so signing with a zero placeholder is safe
    let sig = key.sign(&h.signed_bytes());
    h.signature = sig.to_bytes();

    let mut out = Vec::with_capacity(HEADER_SIZE + payload.len());
    out.extend_from_slice(&h.encode());
    out.extend_from_slice(payload);
    Ok(out)
}

#[derive(Debug, PartialEq, Eq)]
pub enum VerifyError {
    BadPublicKey,
    Header(HeaderError),
    SizeMismatch { expected: usize, actual: usize },
    DigestMismatch,
    BadSignature,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyError::BadPublicKey => write!(f, "invalid public key"),
            VerifyError::Header(e) => write!(f, "header: {e}"),
            VerifyError::SizeMismatch { expected, actual } => {
                write!(f, "image is {actual} bytes, header says {expected}")
            }
            VerifyError::DigestMismatch => write!(f, "payload SHA-256 does not match header digest"),
            VerifyError::BadSignature => write!(f, "Ed25519 signature is invalid"),
        }
    }
}

impl std::error::Error for VerifyError {}

/// full host-side verification, in the order of the header spec: structural checks, size, SHA-256 digest, then the signature
pub fn verify_image(pubkey: &[u8; 32], image: &[u8]) -> Result<Header, VerifyError> {
    let vk = VerifyingKey::from_bytes(pubkey).map_err(|_| VerifyError::BadPublicKey)?;
    let h = Header::decode(image).map_err(VerifyError::Header)?;

    let expected = HEADER_SIZE + h.payload_size as usize;
    if image.len() != expected {
        return Err(VerifyError::SizeMismatch { expected, actual: image.len() });
    }
    if sha256(&image[HEADER_SIZE..]) != h.image_digest {
        return Err(VerifyError::DigestMismatch);
    }
    let sig = Signature::from_bytes(&h.signature);
    vk.verify_strict(&h.signed_bytes(), &sig)
        .map_err(|_| VerifyError::BadSignature)?;
    Ok(h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::IMAGE_TYPE_APP;
    use crate::hexutil::to_hex;

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn pubkey(k: &SigningKey) -> [u8; 32] {
        k.verifying_key().to_bytes()
    }

    fn params() -> ImageParams {
        ImageParams {
            image_type: IMAGE_TYPE_APP,
            security_version: 5,
            load_address: 0x4002_0100,
            entry_point: 0x4002_0100,
            nonce_salt: [0xAB; 16],
        }
    }

    #[test]
    fn sha256_abc() {
        assert_eq!(
            to_hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn build_then_verify_ok() {
        let k = key();
        let payload = b"hello firmware";
        let img = build_image(&params(), payload, &k).unwrap();
        assert_eq!(img.len(), HEADER_SIZE + payload.len());
        let h = verify_image(&pubkey(&k), &img).unwrap();
        assert_eq!(h.payload_size, payload.len() as u32);
        assert_eq!(h.image_digest, sha256(payload));
    }

    #[test]
    fn tampered_payload_is_rejected() {
        let k = key();
        let mut img = build_image(&params(), b"hello firmware", &k).unwrap();
        img[HEADER_SIZE] ^= 0x01;
        assert_eq!(verify_image(&pubkey(&k), &img), Err(VerifyError::DigestMismatch));
    }

    #[test]
    fn truncated_image_is_rejected() {
        let k = key();
        let img = build_image(&params(), b"hello firmware", &k).unwrap();
        let r = verify_image(&pubkey(&k), &img[..img.len() - 1]);
        assert!(matches!(r, Err(VerifyError::SizeMismatch { .. })));
    }

    #[test]
    fn wrong_key_is_rejected() {
        let k = key();
        let other = SigningKey::from_bytes(&[8u8; 32]);
        let img = build_image(&params(), b"hello firmware", &k).unwrap();
        assert_eq!(verify_image(&pubkey(&other), &img), Err(VerifyError::BadSignature));
    }

    #[test]
    fn crc_fixed_header_tamper_is_rejected() {
        /// an attacker who edits a signed field and recomputes the CRC (the CRC is not a security feature) is still caught by the
        /// signature
        let k = key();
        let img = build_image(&params(), b"payload", &k).unwrap();
        let mut h = Header::decode(&img).unwrap();
        h.security_version = 0; // try to dodge the rollback counter
        let mut forged = h.encode().to_vec();
        forged.extend_from_slice(&img[HEADER_SIZE..]);
        assert_eq!(verify_image(&pubkey(&k), &forged), Err(VerifyError::BadSignature));
    }

    #[test]
    fn empty_payload_is_rejected() {
        assert_eq!(build_image(&params(), b"", &key()), Err(BuildError::EmptyPayload));
    }

    #[test]
    fn oversized_payload_is_rejected() {
        let big = vec![0u8; MAX_PAYLOAD as usize + 1];
        assert_eq!(
            build_image(&params(), &big, &key()),
            Err(BuildError::PayloadTooLarge(big.len()))
        );
    }

    #[test]
    fn signing_is_deterministic() {
        let k = key();
        let a = build_image(&params(), b"same input", &k).unwrap();
        let b = build_image(&params(), b"same input", &k).unwrap();
        assert_eq!(a, b);
    }
}
