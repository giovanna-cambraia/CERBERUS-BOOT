//! The crypto engine, checked against published test vectors and against every
//! image in the CLI's vector set. Runs only with `--features crypto`.
#![cfg(feature = "crypto")]

use cerberus_bl2::crypto::DalekCrypto;
use cerberus_bl2::header::{IMAGE_TYPE_APP, IMAGE_TYPE_BL2};
use cerberus_bl2::memmap::{BL2_FLASH, SLOT_A, SLOT_B, SRAM};
use cerberus_bl2::verify::{verify_image, CryptoEngine, Policy, Region};
use cerberus_cli::vectors::{generate, test_public_key};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn arr<const N: usize>(s: &str) -> [u8; N] {
    unhex(s).try_into().unwrap()
}

// RFC 8032 section 7.1, TEST 1 (empty message)
const RFC_PK: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
const RFC_SIG: &str = "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155\
                       5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b";

#[test]
fn sha256_known_answers() {
    let c = DalekCrypto;
    assert_eq!(
        c.sha256(b""),
        arr::<32>("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
    );
    assert_eq!(
        c.sha256(b"abc"),
        arr::<32>("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}

#[test]
fn ed25519_rfc8032_test1() {
    let c = DalekCrypto;
    let pk: [u8; 32] = arr(RFC_PK);
    let sig: [u8; 64] = arr(RFC_SIG);

    assert!(c.ed25519_verify(&pk, b"", &sig));

    // any change to message, signature or key must fail
    assert!(!c.ed25519_verify(&pk, b"\0", &sig));
    let mut bad_sig = sig;
    bad_sig[0] ^= 0x01;
    assert!(!c.ed25519_verify(&pk, b"", &bad_sig));
    let mut bad_pk = pk;
    bad_pk[0] ^= 0x01;
    assert!(!c.ed25519_verify(&bad_pk, b"", &sig));
}

#[test]
fn bogus_public_key_never_verifies() {
    let c = DalekCrypto;
    let sig: [u8; 64] = arr(RFC_SIG);
    assert!(!c.ed25519_verify(&[0xFF; 32], b"", &sig));
}

#[test]
fn weak_keys_are_rejected() {
    // the identity point as public key, with R = identity and S = 0, satisfies
    // the verification equation for ANY message in non-strict verification;
    // verify_strict must refuse it
    let c = DalekCrypto;
    let mut identity = [0u8; 32];
    identity[0] = 1;
    let mut sig = [0u8; 64];
    sig[0] = 1;
    for msg in [&b""[..], &b"anything"[..], &[0u8; 100][..]] {
        assert!(!c.ed25519_verify(&identity, msg, &sig));
    }
}

#[test]
fn engine_agrees_with_every_vector() {
    let c = DalekCrypto;
    let pk = test_public_key();
    let otp_hash = c.sha256(&pk);
    let regions: [Region; 4] = [BL2_FLASH, SLOT_A, SLOT_B, SRAM];

    for v in generate() {
        let ty = if v.file.starts_with("valid_bl2") { IMAGE_TYPE_BL2 } else { IMAGE_TYPE_APP };
        let policy = Policy {
            expected_type: ty,
            allowed_load: &regions,
            rollback_counter: 0,
            public_key: &pk,
            otp_key_hash: &otp_hash,
        };
        match verify_image(&v.bytes, &policy, &c) {
            Ok(_) => assert_eq!(v.expect, "ok", "{} was accepted", v.file),
            Err(e) => {
                assert_eq!(e.code(), v.expect, "{}", v.file);
                assert_eq!(Some(e.step()), v.step, "{}: wrong step", v.file);
            }
        }
    }
}
