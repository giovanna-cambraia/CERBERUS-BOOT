//! Kani harnesses for BL2 (REQ-004, ADR-006).
//!
//! Compiled only by `cargo kani` (see lib.rs). Each harness runs the real code
//! on symbolic inputs and checks that it cannot panic and that the stated
//! property holds for every input within the bounds given in ADR-006.
//!
//!   make kani                                   all harnesses
//!   cd firmware/bl2_rust && cargo kani --harness <name>

use core::cell::Cell;

use crate::boot::{select_boot, BootMode, Config, SlotVerifier};
use crate::error::Error;
use crate::flash::{FlashError, FlashMemory};
use crate::header::{
    crc32, Header, HDR_VERSION, HEADER_SIZE, IMAGE_TYPE_APP, IMAGE_TYPE_BL2, MAGIC, MAX_PAYLOAD,
    OFF_CRC, OFF_MAGIC, OFF_RESERVED, OFF_VERSION,
};
use crate::nv::{NvState, NvStore, Slot, SlotStatus, RECORD_SIZE};
use crate::verify::{ct_eq, verify_image, CryptoEngine, Policy, Region};

fn le16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn le32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

// ---- the header parser -------------------------------------------------------

/// `Header::parse` cannot panic on any input of up to 260 bytes (including
/// inputs shorter than a header), and whatever it accepts satisfies every rule.
#[kani::proof]
#[kani::unwind(260)]
fn verify_header_parsing() {
    const MAX_INPUT: usize = HEADER_SIZE + 4;
    let bytes: [u8; MAX_INPUT] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= MAX_INPUT);
    let input = &bytes[..len];
    let expected: u16 = kani::any();

    let result = Header::parse(input, expected);

    if let Ok(h) = result {
        assert!(len >= HEADER_SIZE);
        assert!(le32(&bytes, OFF_MAGIC) == MAGIC);
        assert!(le16(&bytes, OFF_VERSION) == HDR_VERSION);
        assert!(le32(&bytes, OFF_CRC) == crc32(&bytes[..OFF_CRC]));
        assert!(bytes[OFF_RESERVED..OFF_CRC].iter().all(|&b| b == 0));
        assert!(h.image_type == expected);
        assert!(h.image_type == IMAGE_TYPE_BL2 || h.image_type == IMAGE_TYPE_APP);
        assert!(h.payload_size >= 1 && h.payload_size <= MAX_PAYLOAD);
    }

    kani::cover!(result.is_ok(), "a header can be accepted");
    kani::cover!(len < HEADER_SIZE, "an input shorter than a header is possible");
}

/// The checks run in the documented order: magic first, then version.
#[kani::proof]
#[kani::unwind(260)]
fn verify_check_order() {
    let bytes: [u8; HEADER_SIZE] = kani::any();
    let expected: u16 = kani::any();

    let result = Header::parse(&bytes, expected);

    if le32(&bytes, OFF_MAGIC) != MAGIC {
        assert!(result.err() == Some(Error::BadMagic));
    } else if le16(&bytes, OFF_VERSION) != HDR_VERSION {
        assert!(result.err() == Some(Error::UnsupportedVersion));
    }

    kani::cover!(
        le32(&bytes, OFF_MAGIC) == MAGIC && le16(&bytes, OFF_VERSION) != HDR_VERSION,
        "the version check is reachable"
    );
}

// ---- the verification pipeline ---------------------------------------------

/// A crypto engine that answers arbitrarily and remembers what the signature
/// check returned. The proof therefore holds whatever the real engine says.
struct SpyCrypto {
    verified: Cell<Option<bool>>,
}

impl CryptoEngine for SpyCrypto {
    fn sha256(&self, _data: &[u8]) -> [u8; 32] {
        kani::any()
    }

    fn ed25519_verify(&self, _pk: &[u8; 32], _msg: &[u8], _sig: &[u8; 64]) -> bool {
        let v: bool = kani::any();
        self.verified.set(Some(v));
        v
    }
}

/// `verify_image` cannot panic on any image of up to 264 bytes, under any
/// policy, and it accepts an image only after a successful signature check.
#[kani::proof]
#[kani::unwind(260)]
fn verify_image_accepts_only_verified_images() {
    const MAX: usize = HEADER_SIZE + 8;
    let bytes: [u8; MAX] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= MAX);
    let image = &bytes[..len];

    let regions = [
        Region { start: kani::any(), end: kani::any() },
        Region { start: kani::any(), end: kani::any() },
    ];
    let public_key: [u8; 32] = kani::any();
    let otp_hash: [u8; 32] = kani::any();
    let policy = Policy {
        expected_type: kani::any(),
        allowed_load: &regions,
        rollback_counter: kani::any(),
        public_key: &public_key,
        otp_key_hash: &otp_hash,
    };
    let crypto = SpyCrypto { verified: Cell::new(None) };

    let result = verify_image(image, &policy, &crypto);

    if let Ok(h) = result {
        // Accepted only after the signature check said yes.
        assert!(crypto.verified.get() == Some(true));
        // Exact size, version, type.
        assert!(image.len() == HEADER_SIZE + h.payload_size as usize);
        assert!(h.security_version >= policy.rollback_counter);
        assert!(h.image_type == policy.expected_type);
        // Entry point inside the payload, and the payload inside a region.
        let end = u64::from(h.load_address) + u64::from(h.payload_size);
        assert!(h.entry_point >= h.load_address);
        assert!(u64::from(h.entry_point) < end);
        assert!(regions
            .iter()
            .any(|r| h.load_address >= r.start && end <= u64::from(r.end)));
    }

    kani::cover!(result.is_ok(), "an image can be accepted");
}

/// The constant-time comparison is plain equality.
#[kani::proof]
#[kani::unwind(40)]
fn verify_ct_eq_is_equality() {
    let a: [u8; 32] = kani::any();
    let b: [u8; 32] = kani::any();
    assert!(ct_eq(&a, &b) == (a == b));

    let n: usize = kani::any();
    kani::assume(n < 32);
    assert!(!ct_eq(&a, &b[..n]));
}

// ---- the slot-state record -------------------------------------------------

/// If a record decodes, encoding it gives back exactly the same bytes: no
/// record has two readings, and nothing outside the fields is ignored.
#[kani::proof]
#[kani::unwind(40)]
fn verify_nv_decode_is_canonical() {
    let bytes: [u8; RECORD_SIZE] = kani::any();

    let decoded = NvState::decode(&bytes);

    if let Some(state) = decoded {
        assert!(state.encode() == bytes);
    }
    kani::cover!(decoded.is_some(), "a record can decode");
}

fn any_status() -> SlotStatus {
    match kani::any::<u8>() % 6 {
        0 => SlotStatus::Empty,
        1 => SlotStatus::Pending,
        2 => SlotStatus::Testing,
        3 => SlotStatus::Confirmed,
        4 => SlotStatus::Invalid,
        _ => SlotStatus::Broken,
    }
}

/// Every valid state survives encode and decode.
#[kani::proof]
#[kani::unwind(40)]
fn verify_nv_round_trip() {
    let state = NvState {
        seq: kani::any(),
        active: if kani::any() { Slot::A } else { Slot::B },
        slot_a: any_status(),
        slot_b: any_status(),
        trials_left: kani::any(),
    };
    assert!(NvState::decode(&state.encode()) == Some(state));
}

// ---- the slot state machine ------------------------------------------------

const TINY_SECTOR: usize = 64;

/// Flash with 64-byte sectors: the NV code only ever touches the first
/// 32 bytes of a sector, so the logic is the same as with 4 KB sectors, and the
/// erase loop is short enough to unroll. Every access is bounds-checked, so the
/// model itself cannot panic.
struct TinyFlash {
    mem: [u8; 2 * TINY_SECTOR],
}

impl FlashMemory for TinyFlash {
    const SECTOR_SIZE: u32 = TINY_SECTOR as u32;

    fn read(&self, addr: u32, buf: &mut [u8]) -> Result<(), FlashError> {
        let a = addr as usize;
        let end = a.checked_add(buf.len()).ok_or(FlashError::OutOfRange)?;
        if end > self.mem.len() {
            return Err(FlashError::OutOfRange);
        }
        buf.copy_from_slice(&self.mem[a..end]);
        Ok(())
    }

    fn erase_sector(&mut self, addr: u32) -> Result<(), FlashError> {
        let a = addr as usize;
        if a % TINY_SECTOR != 0 || a + TINY_SECTOR > self.mem.len() {
            return Err(FlashError::OutOfRange);
        }
        for b in &mut self.mem[a..a + TINY_SECTOR] {
            *b = 0xFF;
        }
        Ok(())
    }

    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashError> {
        let a = addr as usize;
        let end = a.checked_add(data.len()).ok_or(FlashError::OutOfRange)?;
        if end > self.mem.len() {
            return Err(FlashError::OutOfRange);
        }
        for (i, &d) in data.iter().enumerate() {
            self.mem[a + i] &= d;
        }
        Ok(())
    }
}

/// A verifier that answers arbitrarily and remembers its last answer per slot.
struct SymbolicVerifier {
    last: [Option<bool>; 2],
}

impl SlotVerifier for SymbolicVerifier {
    fn verify_slot(&mut self, slot: Slot) -> Result<u32, Error> {
        let ok: bool = kani::any();
        self.last[slot as usize] = Some(ok);
        if ok {
            Ok(kani::any())
        } else {
            Err(Error::BadSignature)
        }
    }
}

/// For ANY contents of the NV flash (an attacker may have written anything
/// there), any trial count and any verifier answers, `select_boot` cannot
/// panic, and it only picks a slot that verified. A trial boot never asks for
/// the counter to be raised; a normal boot always does.
#[kani::proof]
#[kani::unwind(70)]
fn verify_select_boot_only_boots_verified_slots() {
    let mut store = NvStore::new(TinyFlash { mem: kani::any() }, 0);
    let mut verifier = SymbolicVerifier { last: [None, None] };
    let cfg = Config { trial_boots: kani::any() };

    let result = select_boot(&mut store, &mut verifier, &cfg);

    if let Ok(plan) = result {
        assert!(verifier.last[plan.slot as usize] == Some(true));
        match plan.mode {
            BootMode::Trial => assert!(plan.counter_target.is_none()),
            BootMode::Normal => assert!(plan.counter_target.is_some()),
        }
    }

    kani::cover!(
        matches!(result, Ok(p) if p.mode == BootMode::Trial),
        "a trial boot is possible"
    );
    kani::cover!(
        matches!(result, Ok(p) if p.mode == BootMode::Normal),
        "a normal boot is possible"
    );
}