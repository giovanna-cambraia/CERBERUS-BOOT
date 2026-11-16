//! Boot flow tests with mock hardware and real signed images.

use cerberus_bl2::boot::{BootMode, Config, HaltReason};
use cerberus_bl2::flash::{FlashError, FlashMemory};
use cerberus_bl2::flow::{boot_flow, HaltCause, JumpTarget, Outcome};
use cerberus_bl2::memmap::{NV_STATE, SLOT_A, SLOT_B};
use cerberus_bl2::nv::{NvStore, Slot, SlotStatus};
use cerberus_bl2::platform::{RootOfTrust, SlotStorage};
use cerberus_bl2::verify::CryptoEngine;
use cerberus_cli::header::IMAGE_TYPE_APP;
use cerberus_cli::image::{build_image, ImageParams};
use cerberus_cli::vectors::{test_public_key, test_signing_key};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

const BASE: u32 = NV_STATE.start;
const SECTOR: usize = 4096;
const APP_A_ENTRY: u32 = SLOT_A.start + 0x100;
const APP_B_ENTRY: u32 = SLOT_B.start + 0x100;

// ---- mocks -------------------------------------------------------------------

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

struct MockRot {
    locked: bool,
    hash: [u8; 32],
    counter: u32,
    reject_burn: bool,
    burns: Vec<u32>,
}

impl RootOfTrust for MockRot {
    fn locked(&self) -> bool {
        self.locked
    }
    fn key_hash(&self) -> [u8; 32] {
        self.hash
    }
    fn counter(&self) -> u32 {
        self.counter
    }
    fn burn_counter(&mut self, value: u32) {
        self.burns.push(value);
        if !self.reject_burn && value > self.counter {
            self.counter = value;
        }
    }
}

struct MockSlots {
    a: Vec<u8>,
    b: Vec<u8>,
}

impl MockSlots {
    fn empty() -> Self {
        let n = (SLOT_A.end - SLOT_A.start) as usize;
        MockSlots { a: vec![0xFF; n], b: vec![0xFF; n] }
    }

    fn put(&mut self, slot: Slot, image: &[u8]) {
        let area = match slot {
            Slot::A => &mut self.a,
            Slot::B => &mut self.b,
        };
        area[..image.len()].copy_from_slice(image);
    }
}

impl SlotStorage for MockSlots {
    fn slot_bytes(&self, slot: Slot) -> &[u8] {
        match slot {
            Slot::A => &self.a,
            Slot::B => &self.b,
        }
    }
}

struct MockFlash {
    mem: Vec<u8>,
}

impl MockFlash {
    fn new() -> Self {
        MockFlash { mem: vec![0xFF; 2 * SECTOR] }
    }

    fn offset(addr: u32, len: usize, total: usize) -> Result<usize, FlashError> {
        let off = addr.checked_sub(BASE).ok_or(FlashError::OutOfRange)? as usize;
        if off + len > total {
            return Err(FlashError::OutOfRange);
        }
        Ok(off)
    }
}

impl FlashMemory for MockFlash {
    const SECTOR_SIZE: u32 = SECTOR as u32;

    fn read(&self, addr: u32, buf: &mut [u8]) -> Result<(), FlashError> {
        let i = Self::offset(addr, buf.len(), self.mem.len())?;
        buf.copy_from_slice(&self.mem[i..i + buf.len()]);
        Ok(())
    }

    fn erase_sector(&mut self, addr: u32) -> Result<(), FlashError> {
        let i = Self::offset(addr, SECTOR, self.mem.len())?;
        self.mem[i..i + SECTOR].fill(0xFF);
        Ok(())
    }

    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashError> {
        let i = Self::offset(addr, data.len(), self.mem.len())?;
        for (k, &d) in data.iter().enumerate() {
            self.mem[i + k] &= d;
        }
        Ok(())
    }
}

// ---- helpers -----------------------------------------------------------------

struct Fixture {
    rot: MockRot,
    slots: MockSlots,
    nv: NvStore<MockFlash>,
}

fn fixture() -> Fixture {
    Fixture {
        rot: MockRot {
            locked: true,
            hash: Sha256::digest(test_public_key()).into(),
            counter: 0,
            reject_burn: false,
            burns: Vec::new(),
        },
        slots: MockSlots::empty(),
        nv: NvStore::new(MockFlash::new(), BASE),
    }
}

/// a signed app image linked to run from `slot` (XIP), with `secver`
fn image_for(slot: Slot, secver: u32) -> Vec<u8> {
    let base = match slot {
        Slot::A => SLOT_A.start,
        Slot::B => SLOT_B.start,
    };
    let load = base + 0x100;
    let p = ImageParams {
        image_type: IMAGE_TYPE_APP,
        security_version: secver,
        load_address: load,
        entry_point: load,
        nonce_salt: [secver as u8; 16],
    };
    build_image(&p, &[0xA5; 64], &test_signing_key()).unwrap()
}

fn run(fx: &mut Fixture) -> Outcome {
    boot_flow(
        &mut fx.rot,
        &fx.slots,
        &mut fx.nv,
        &HostCrypto,
        &test_public_key(),
        &Config::default(),
    )
}

fn jump(slot: Slot, mode: BootMode, entry_point: u32) -> Outcome {
    Outcome::Jump(JumpTarget { slot, mode, entry_point })
}

fn no_bootable_image() -> Outcome {
    Outcome::Halt(HaltCause::Boot(HaltReason::NoBootableImage))
}

// ---- tests -------------------------------------------------------------------

#[test]
fn unprovisioned_device_halts() {
    let mut fx = fixture();
    fx.slots.put(Slot::A, &image_for(Slot::A, 1));
    fx.rot.locked = false;
    assert_eq!(run(&mut fx), Outcome::Halt(HaltCause::NotProvisioned));
}

#[test]
fn key_hash_mismatch_halts_and_leaves_nv_alone() {
    let mut fx = fixture();
    fx.slots.put(Slot::A, &image_for(Slot::A, 1));
    fx.rot.hash[0] ^= 1;
    assert_eq!(run(&mut fx), Outcome::Halt(HaltCause::KeyHashMismatch));
    // a platform fault must not mark good slots invalid
    assert_eq!(fx.nv.load().unwrap(), None);
}

#[test]
fn normal_boot_from_slot_a_burns_the_counter_once() {
    let mut fx = fixture();
    fx.slots.put(Slot::A, &image_for(Slot::A, 3));
    assert_eq!(run(&mut fx), jump(Slot::A, BootMode::Normal, APP_A_ENTRY));
    assert_eq!(fx.rot.burns, vec![3]);
    assert_eq!(fx.rot.counter, 3);

    // same image again: the counter is already there, no second burn
    assert_eq!(run(&mut fx), jump(Slot::A, BootMode::Normal, APP_A_ENTRY));
    assert_eq!(fx.rot.burns, vec![3]);
}

#[test]
fn update_trial_and_confirm() {
    let mut fx = fixture();
    fx.slots.put(Slot::A, &image_for(Slot::A, 1));
    assert_eq!(run(&mut fx), jump(Slot::A, BootMode::Normal, APP_A_ENTRY));

    // the OTA agent stages version 2 into slot B
    fx.slots.put(Slot::B, &image_for(Slot::B, 2));
    fx.nv.begin_update(Slot::B).unwrap();
    fx.nv.mark_pending(Slot::B).unwrap();

    // trial boot: the counter stays at 1 so A remains a valid fallback
    assert_eq!(run(&mut fx), jump(Slot::B, BootMode::Trial, APP_B_ENTRY));
    assert_eq!(fx.rot.burns, vec![1]);

    // after confirmation the next boot is normal and burns the counter to 2
    fx.nv.confirm(Slot::B).unwrap();
    assert_eq!(run(&mut fx), jump(Slot::B, BootMode::Normal, APP_B_ENTRY));
    assert_eq!(fx.rot.burns, vec![1, 2]);
}

#[test]
fn image_linked_for_the_other_slot_is_rejected() {
    let mut fx = fixture();
    fx.slots.put(Slot::A, &image_for(Slot::A, 1));
    // properly signed, but linked for slot A addresses and placed in slot B
    fx.slots.put(Slot::B, &image_for(Slot::A, 2));
    fx.nv.mark_pending(Slot::B).unwrap();

    assert_eq!(run(&mut fx), jump(Slot::A, BootMode::Normal, APP_A_ENTRY));
    let st = fx.nv.load().unwrap().unwrap();
    assert_eq!(st.status(Slot::B), SlotStatus::Invalid);
}

#[test]
fn rollback_counter_blocks_an_old_image() {
    let mut fx = fixture();
    fx.rot.counter = 2;
    fx.slots.put(Slot::A, &image_for(Slot::A, 1));
    assert_eq!(run(&mut fx), no_bootable_image());
    assert_eq!(fx.nv.load().unwrap().unwrap().status(Slot::A), SlotStatus::Invalid);
}

#[test]
fn counter_burn_failure_halts() {
    let mut fx = fixture();
    fx.rot.reject_burn = true;
    fx.slots.put(Slot::A, &image_for(Slot::A, 5));
    assert_eq!(run(&mut fx), Outcome::Halt(HaltCause::CounterBurnFailed));
}

#[test]
fn tampered_payload_is_not_booted() {
    let mut fx = fixture();
    let mut img = image_for(Slot::A, 1);
    img[256] ^= 0x01; // first payload byte
    fx.slots.put(Slot::A, &img);
    assert_eq!(run(&mut fx), no_bootable_image());
    assert_eq!(fx.nv.load().unwrap().unwrap().status(Slot::A), SlotStatus::Invalid);
}

#[test]
fn empty_slots_halt() {
    let mut fx = fixture();
    assert_eq!(run(&mut fx), no_bootable_image());
}
