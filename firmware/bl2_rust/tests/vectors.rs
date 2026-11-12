//! Conformance tests: run every file in test/vectors through the BL2 pipeline.
//! Regenerate the vectors with `make vectors` if they are missing.

use cerberus_bl2::error::Error;
use cerberus_bl2::header::{IMAGE_TYPE_APP, IMAGE_TYPE_BL2};
use cerberus_bl2::memmap::{BL2_FLASH, SLOT_A, SLOT_B, SRAM};
use cerberus_bl2::verify::{verify_image, CryptoEngine, Policy, Region};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

struct HostCrypto;

impl CryptoEngine for HostCrypto {
    fn sha256(&self, data: &[u8]) -> [u8; 32] {
        Sha256::digest(data).into()
    }

    fn ed25519_verify(&self, pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
        match VerifyingKey::from_bytes(pk) {
            Ok(vk) => vk.verify_strict(msg, &Signature::from_bytes(sig)).is_ok(),
            Err(_) => false,
        }
    }
}

// Q2 (XIP vs. copy to SRAM) is still open, so the conformance policy allows every plausible load region
const CONFORMANCE_REGIONS: [Region; 4] = [BL2_FLASH, SLOT_A, SLOT_B, SRAM];

#[derive(Default)]
struct Entry {
    file: String,
    expect: String,
    step: Option<u8>,
}

struct Manifest {
    pubkey: [u8; 32],
    pubkey_sha256: [u8; 32],
    entries: Vec<Entry>,
}

fn vec_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/vectors")
}

fn read_vec(name: &str) -> Vec<u8> {
    fs::read(vec_dir().join(name))
        .unwrap_or_else(|e| panic!("{name}: {e} (run `make vectors` first)"))
}

fn unhex32(s: &str) -> [u8; 32] {
    assert_eq!(s.len(), 64, "expected 64 hex digits");
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("hex");
    }
    out
}

fn load_manifest() -> Manifest {
    let text = fs::read_to_string(vec_dir().join("manifest.toml"))
        .expect("manifest.toml missing (run `make vectors` first)");
    let mut pubkey = None;
    let mut pubkey_sha256 = None;
    let mut entries = Vec::new();
    let mut cur: Option<Entry> = None;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line == "[meta]" {
            continue;
        }
        if line == "[[vector]]" {
            if let Some(e) = cur.take() {
                entries.push(e);
            }
            cur = Some(Entry::default());
            continue;
        }
        let (k, v) = line.split_once('=').expect("key = value line");
        let (k, v) = (k.trim(), v.trim().trim_matches('"'));
        match cur.as_mut() {
            None => match k {
                "pubkey" => pubkey = Some(unhex32(v)),
                "pubkey_sha256" => pubkey_sha256 = Some(unhex32(v)),
                _ => {}
            },
            Some(e) => match k {
                "file" => e.file = v.to_string(),
                "expect" => e.expect = v.to_string(),
                "step" => e.step = Some(v.parse().expect("step number")),
                _ => {}
            },
        }
    }
    if let Some(e) = cur.take() {
        entries.push(e);
    }
    Manifest {
        pubkey: pubkey.expect("pubkey in manifest"),
        pubkey_sha256: pubkey_sha256.expect("pubkey_sha256 in manifest"),
        entries,
    }
}

fn policy<'a>(m: &'a Manifest, expected_type: u16, regions: &'a [Region], counter: u32) -> Policy<'a> {
    Policy {
        expected_type,
        allowed_load: regions,
        rollback_counter: counter,
        public_key: &m.pubkey,
        otp_key_hash: &m.pubkey_sha256,
    }
}

#[test]
fn manifest_vectors_match_expectations() {
    let m = load_manifest();
    assert!(!m.entries.is_empty(), "manifest has no vectors");
    for e in &m.entries {
        let img = read_vec(&e.file);
        let ty = if e.file.starts_with("valid_bl2") { IMAGE_TYPE_BL2 } else { IMAGE_TYPE_APP };
        let p = policy(&m, ty, &CONFORMANCE_REGIONS, 0);
        match verify_image(&img, &p, &HostCrypto) {
            Ok(_) => assert_eq!(e.expect, "ok", "{} was accepted", e.file),
            Err(err) => {
                assert_eq!(err.code(), e.expect, "{}", e.file);
                assert_eq!(Some(err.step()), e.step, "{}: rejected at the wrong step", e.file);
            }
        }
    }
}

#[test]
fn rollback_counter_is_enforced() {
    let m = load_manifest();
    let v1 = read_vec("valid_app_secver1.img");
    let v2 = read_vec("valid_app_secver2.img");

    let p = policy(&m, IMAGE_TYPE_APP, &CONFORMANCE_REGIONS, 2);
    assert_eq!(verify_image(&v1, &p, &HostCrypto).unwrap_err(), Error::RollbackRejected);
    assert!(verify_image(&v2, &p, &HostCrypto).is_ok());

    let p = policy(&m, IMAGE_TYPE_APP, &CONFORMANCE_REGIONS, 3);
    assert_eq!(verify_image(&v2, &p, &HostCrypto).unwrap_err(), Error::RollbackRejected);
}

#[test]
fn strict_region_policy_blocks_redirected_load_address() {
    let m = load_manifest();
    let only_slot_a = [SLOT_A];

    // the forged image (load address moved to SRAM) is caught at step 7 under a policy that does not allow SRAM, before the signature
    // check
    let forged = read_vec("forged_load_address.img");
    let p = policy(&m, IMAGE_TYPE_APP, &only_slot_a, 0);
    assert_eq!(verify_image(&forged, &p, &HostCrypto).unwrap_err(), Error::BadLoadAddress);

    // a normal app image still passes, and a BL2 image does not fit slot A
    let app = read_vec("valid_app_secver1.img");
    assert!(verify_image(&app, &p, &HostCrypto).is_ok());
    let bl2 = read_vec("valid_bl2_secver1.img");
    let p = policy(&m, IMAGE_TYPE_BL2, &only_slot_a, 0);
    assert_eq!(verify_image(&bl2, &p, &HostCrypto).unwrap_err(), Error::BadLoadAddress);
}

#[test]
fn image_type_must_match_the_slot() {
    let m = load_manifest();
    let app = read_vec("valid_app_secver1.img");
    let bl2 = read_vec("valid_bl2_secver1.img");

    let p = policy(&m, IMAGE_TYPE_BL2, &CONFORMANCE_REGIONS, 0);
    assert_eq!(verify_image(&app, &p, &HostCrypto).unwrap_err(), Error::WrongImageType);
    let p = policy(&m, IMAGE_TYPE_APP, &CONFORMANCE_REGIONS, 0);
    assert_eq!(verify_image(&bl2, &p, &HostCrypto).unwrap_err(), Error::WrongImageType);
}

#[test]
fn public_key_must_match_the_otp_hash() {
    let m = load_manifest();
    let img = read_vec("valid_app_secver1.img");

    // OTP holds a different hash
    let mut bad_hash = m.pubkey_sha256;
    bad_hash[0] ^= 0x01;
    let p = Policy {
        expected_type: IMAGE_TYPE_APP,
        allowed_load: &CONFORMANCE_REGIONS,
        rollback_counter: 0,
        public_key: &m.pubkey,
        otp_key_hash: &bad_hash,
    };
    assert_eq!(verify_image(&img, &p, &HostCrypto).unwrap_err(), Error::KeyHashMismatch);

    // the embedded key was swapped but the OTP hash is the genuine one
    let mut bad_key = m.pubkey;
    bad_key[0] ^= 0x01;
    let p = Policy {
        expected_type: IMAGE_TYPE_APP,
        allowed_load: &CONFORMANCE_REGIONS,
        rollback_counter: 0,
        public_key: &bad_key,
        otp_key_hash: &m.pubkey_sha256,
    };
    assert_eq!(verify_image(&img, &p, &HostCrypto).unwrap_err(), Error::KeyHashMismatch);
}
