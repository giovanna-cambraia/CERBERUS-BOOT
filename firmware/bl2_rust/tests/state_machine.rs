//! State machine tests on a simulated NOR flash, including power-loss sweeps.

use cerberus_bl2::boot::{select_boot, BootMode, BootPlan, Config, HaltReason, SlotVerifier};
use cerberus_bl2::error::Error;
use cerberus_bl2::flash::{FlashError, FlashMemory};
use cerberus_bl2::memmap::NV_STATE;
use cerberus_bl2::nv::{NvState, NvStore, Slot, SlotStatus, UpdateError, RECORD_SIZE};

const BASE: u32 = NV_STATE.start;
const SECTOR: usize = 4096;
/// flash operations needed for one commit: one sector erase plus one record
const COMMIT_OPS: usize = SECTOR + RECORD_SIZE;

/// NOR flash model. `budget` = number of byte operations left before the
/// power is cut (None = no cut); erase and program are modelled byte by byte,
/// so a cut can land anywhere inside them
#[derive(Clone)]
struct MockFlash {
    base: u32,
    mem: Vec<u8>,
    budget: Option<usize>,
}

impl MockFlash {
    fn new(base: u32, size: usize) -> Self {
        MockFlash { base, mem: vec![0xFF; size], budget: None }
    }

    fn spend(&mut self) -> Result<(), FlashError> {
        match self.budget.as_mut() {
            None => Ok(()),
            Some(0) => Err(FlashError::Device),
            Some(n) => {
                *n -= 1;
                Ok(())
            }
        }
    }

    fn offset(&self, addr: u32, len: usize) -> Result<usize, FlashError> {
        let off = addr.checked_sub(self.base).ok_or(FlashError::OutOfRange)? as usize;
        if off + len > self.mem.len() {
            return Err(FlashError::OutOfRange);
        }
        Ok(off)
    }
}

impl FlashMemory for MockFlash {
    const SECTOR_SIZE: u32 = SECTOR as u32;

    fn read(&self, addr: u32, buf: &mut [u8]) -> Result<(), FlashError> {
        let i = self.offset(addr, buf.len())?;
        buf.copy_from_slice(&self.mem[i..i + buf.len()]);
        Ok(())
    }

    fn erase_sector(&mut self, addr: u32) -> Result<(), FlashError> {
        let i = self.offset(addr, SECTOR)?;
        if i % SECTOR != 0 {
            return Err(FlashError::OutOfRange);
        }
        for b in i..i + SECTOR {
            self.spend()?;
            self.mem[b] = 0xFF;
        }
        Ok(())
    }

    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashError> {
        let i = self.offset(addr, data.len())?;
        for (k, &d) in data.iter().enumerate() {
            self.spend()?;
            self.mem[i + k] &= d; // NOR: programming can only clear bits
        }
        Ok(())
    }
}

struct FakeVerifier {
    a: Result<u32, Error>,
    b: Result<u32, Error>,
}

impl FakeVerifier {
    fn ok(a_secver: u32, b_secver: u32) -> Self {
        FakeVerifier { a: Ok(a_secver), b: Ok(b_secver) }
    }
}

impl SlotVerifier for FakeVerifier {
    fn verify_slot(&mut self, slot: Slot) -> Result<u32, Error> {
        match slot {
            Slot::A => self.a,
            Slot::B => self.b,
        }
    }
}

fn fresh_store() -> NvStore<MockFlash> {
    NvStore::new(MockFlash::new(BASE, 2 * SECTOR), BASE)
}

fn boot(store: &mut NvStore<MockFlash>, v: &mut FakeVerifier) -> Result<BootPlan, HaltReason> {
    select_boot(store, v, &Config::default())
}

fn state(store: &NvStore<MockFlash>) -> NvState {
    store.load().unwrap().expect("a valid record")
}

fn plan(slot: Slot, mode: BootMode, counter_target: Option<u32>) -> BootPlan {
    BootPlan { slot, mode, counter_target }
}

#[test]
fn first_boot_uses_the_factory_state() {
    let mut store = fresh_store();
    let mut v = FakeVerifier::ok(1, 0);
    assert_eq!(boot(&mut store, &mut v), Ok(plan(Slot::A, BootMode::Normal, Some(1))));
}

#[test]
fn update_happy_path() {
    let mut store = fresh_store();
    let mut v = FakeVerifier::ok(1, 2);

    store.begin_update(Slot::B).unwrap();
    store.mark_pending(Slot::B).unwrap();

    // EVALUATE -> TESTING: boots B on trial, counter untouched
    assert_eq!(boot(&mut store, &mut v), Ok(plan(Slot::B, BootMode::Trial, None)));
    let st = state(&store);
    assert_eq!(st.status(Slot::B), SlotStatus::Testing);
    assert_eq!(st.trials_left, 2);
    assert_eq!(st.active, Slot::A);

    // the application confirms
    store.confirm(Slot::B).unwrap();
    let st = state(&store);
    assert_eq!(st.active, Slot::B);
    assert_eq!(st.status(Slot::B), SlotStatus::Confirmed);

    // next boot is normal and asks for the counter to be burned to B's version
    assert_eq!(boot(&mut store, &mut v), Ok(plan(Slot::B, BootMode::Normal, Some(2))));
}

#[test]
fn pending_image_failing_verification_is_invalidated() {
    let mut store = fresh_store();
    let mut v = FakeVerifier::ok(1, 2);
    v.b = Err(Error::BadSignature);

    store.mark_pending(Slot::B).unwrap();
    assert_eq!(boot(&mut store, &mut v), Ok(plan(Slot::A, BootMode::Normal, Some(1))));
    let st = state(&store);
    assert_eq!(st.status(Slot::B), SlotStatus::Invalid);
    assert_eq!(st.status(Slot::A), SlotStatus::Confirmed);
}

#[test]
fn trial_boots_are_limited() {
    let mut store = fresh_store();
    let mut v = FakeVerifier::ok(1, 2);
    store.mark_pending(Slot::B).unwrap();

    // three trial boots (the default), none of them confirmed
    for n in 0..3 {
        let p = boot(&mut store, &mut v).unwrap();
        assert_eq!((p.slot, p.mode), (Slot::B, BootMode::Trial), "trial boot {n}");
    }
    // the fourth reset falls back to A and marks B as broken
    assert_eq!(boot(&mut store, &mut v), Ok(plan(Slot::A, BootMode::Normal, Some(1))));
    assert_eq!(state(&store).status(Slot::B), SlotStatus::Broken);
}

#[test]
fn trial_image_that_stops_verifying_falls_back() {
    let mut store = fresh_store();
    let mut v = FakeVerifier::ok(1, 2);
    store.mark_pending(Slot::B).unwrap();
    assert_eq!(boot(&mut store, &mut v).unwrap().mode, BootMode::Trial);

    v.b = Err(Error::DigestMismatch); // e.g. flash corruption
    assert_eq!(boot(&mut store, &mut v), Ok(plan(Slot::A, BootMode::Normal, Some(1))));
    assert_eq!(state(&store).status(Slot::B), SlotStatus::Invalid);
}

#[test]
fn active_follows_the_slot_that_actually_boots() {
    let mut store = fresh_store();
    let mut v = FakeVerifier::ok(1, 2);
    store.mark_pending(Slot::B).unwrap();
    boot(&mut store, &mut v).unwrap();
    store.confirm(Slot::B).unwrap(); // active = B

    // B goes bad later and the old image is still acceptable
    v.b = Err(Error::BadSignature);
    assert_eq!(boot(&mut store, &mut v), Ok(plan(Slot::A, BootMode::Normal, Some(1))));
    let st = state(&store);
    assert_eq!(st.active, Slot::A); // the OTA agent must now write into B
    assert_eq!(st.status(Slot::B), SlotStatus::Invalid);
}

#[test]
fn rollback_protection_can_leave_nothing_to_boot() {
    let mut store = fresh_store();
    let mut v = FakeVerifier::ok(1, 2);
    store.mark_pending(Slot::B).unwrap();
    boot(&mut store, &mut v).unwrap();
    store.confirm(Slot::B).unwrap();

    // the counter now sits at 2, so A is rejected by the rollback gate,
    // and B has become unverifiable
    v.a = Err(Error::RollbackRejected);
    v.b = Err(Error::BadSignature);
    assert_eq!(boot(&mut store, &mut v), Err(HaltReason::NoBootableImage));
    let st = state(&store);
    assert_eq!((st.slot_a, st.slot_b), (SlotStatus::Invalid, SlotStatus::Invalid));
}

#[test]
fn update_api_guards() {
    let mut store = fresh_store();
    let mut v = FakeVerifier::ok(1, 2);

    // never touch the active slot
    assert_eq!(store.begin_update(Slot::A), Err(UpdateError::NotAllowed));
    assert_eq!(store.mark_pending(Slot::A), Err(UpdateError::NotAllowed));

    // only a slot in Testing can be confirmed
    store.mark_pending(Slot::B).unwrap();
    assert_eq!(store.confirm(Slot::B), Err(UpdateError::NotAllowed));

    // no new update while an image is on trial
    boot(&mut store, &mut v).unwrap();
    assert_eq!(store.begin_update(Slot::B), Err(UpdateError::NotAllowed));

    // after confirming, the passive slot is A; it needs begin_update first
    store.confirm(Slot::B).unwrap();
    assert_eq!(store.mark_pending(Slot::A), Err(UpdateError::NotAllowed));
    store.begin_update(Slot::A).unwrap();
    store.mark_pending(Slot::A).unwrap();
}

#[test]
fn power_loss_during_commit_never_loses_state() {
    let mut store = fresh_store();
    let mut last = store.commit(NvState::factory()).unwrap(); // seq 1, sector 0

    // three rounds, so the sweep hits both sectors as the write target
    for round in 0..3u8 {
        let mut desired = last;
        desired.trials_left = round + 1;
        let mut expected_new = desired;
        expected_new.seq = last.seq + 1;

        for budget in 0..=COMMIT_OPS + 2 {
            let mut flash = store.flash().clone();
            flash.budget = Some(budget);
            let mut cut = NvStore::new(flash, BASE);
            let _ = cut.commit(desired); // may fail: the power went away

            let mut flash = cut.into_flash();
            flash.budget = None; // power is back
            let after = NvStore::new(flash, BASE)
                .load()
                .unwrap()
                .expect("a valid record must always survive");
            assert!(
                after == last || after == expected_new,
                "round {round}, budget {budget}: unexpected state {after:?}"
            );
        }
        last = store.commit(desired).unwrap();
    }
}

#[test]
fn power_loss_during_boot_selection_still_boots() {
    let mut base = fresh_store();
    base.mark_pending(Slot::B).unwrap(); // a confirmed, B pending

    for budget in 0..=COMMIT_OPS + 2 {
        let mut flash = base.flash().clone();
        flash.budget = Some(budget);
        let mut cut = NvStore::new(flash, BASE);
        let mut v = FakeVerifier::ok(1, 2);
        let _ = select_boot(&mut cut, &mut v, &Config::default());

        let mut flash = cut.into_flash();
        flash.budget = None; // power is back, reboot
        let mut store = NvStore::new(flash, BASE);
        let mut v = FakeVerifier::ok(1, 2);
        let p = select_boot(&mut store, &mut v, &Config::default())
            .unwrap_or_else(|e| panic!("budget {budget}: halted with {e:?}"));
        assert_eq!((p.slot, p.mode), (Slot::B, BootMode::Trial), "budget {budget}");
    }
}
