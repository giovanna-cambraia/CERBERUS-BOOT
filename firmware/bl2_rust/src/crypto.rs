//! SHA-256 and Ed25519 for the verification pipeline (ADR-002).
//! Built only with the `crypto` feature.

use crate::verify::CryptoEngine;
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

pub struct DalekCrypto;

impl CryptoEngine for DalekCrypto {
    fn sha256(&self, data: &[u8]) -> [u8; 32] {
        Sha256::digest(data).into()
    }

    /// `verify_strict` also rejects small-order (weak) keys and signature
    /// components, which plain `verify` would accept
    fn ed25519_verify(&self, public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
        match VerifyingKey::from_bytes(public_key) {
            Ok(vk) => vk
                .verify_strict(message, &Signature::from_bytes(signature))
                .is_ok(),
            Err(_) => false,
        }
    }
}
