//! BL2 boot flow: ties the hardware (RoT, slot flash, NV state) to the
//! verification pipeline and the slot state machine.
//! Everything hardware-specific comes in through traits.

use crate::boot::{select_boot, BootMode, Config, HaltReason, SlotVerifier};
use crate::error::Error;
use crate::flash::FlashMemory;
use crate::header::{Header, HEADER_SIZE, IMAGE_TYPE_APP, OFF_PAYLOAD_SIZE};
use crate::memmap::{SLOT_A, SLOT_B};
use crate::nv::{NvStore, Slot};
use crate::platform::{RootOfTrust, SlotStorage};
use crate::verify::{ct_eq, verify_image, CryptoEngine, Policy, Region};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpTarget {
    pub slot: Slot,
    pub mode: BootMode,
    pub entry_point: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaltCause {
    /// the RoT fuses are not committed
    NotProvisioned,
    /// the key embedded in BL2 does not match the OTP hash
    KeyHashMismatch,
    Boot(HaltReason),
    CounterBurnFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Jump(JumpTarget),
    Halt(HaltCause),
}

impl Outcome {
    /// numeric code for the boot report: 0/1 = jump (normal/trial), 0x1x = halt
    pub const fn code(&self) -> u32 {
        match self {
            Outcome::Jump(t) => match t.mode {
                BootMode::Normal => 0,
                BootMode::Trial => 1,
            },
            Outcome::Halt(HaltCause::NotProvisioned) => 0x10,
            Outcome::Halt(HaltCause::KeyHashMismatch) => 0x11,
            Outcome::Halt(HaltCause::Boot(HaltReason::NoBootableImage)) => 0x12,
            Outcome::Halt(HaltCause::Boot(HaltReason::Storage(_))) => 0x13,
            Outcome::Halt(HaltCause::CounterBurnFailed) => 0x14,
        }
    }
}

fn slot_region(slot: Slot) -> Region {
    match slot {
        Slot::A => SLOT_A,
        Slot::B => SLOT_B,
    }
}

/// cut the slot area down to header + declared payload; a bogus declared size is not trusted here: if it exceeds the area, the whole
/// area is passed on and the header parser rejects the size
fn image_extent(area: &[u8]) -> Result<&[u8], Error> {
    if area.len() < HEADER_SIZE {
        return Err(Error::TooShort);
    }
    let declared = u32::from_le_bytes([
        area[OFF_PAYLOAD_SIZE],
        area[OFF_PAYLOAD_SIZE + 1],
        area[OFF_PAYLOAD_SIZE + 2],
        area[OFF_PAYLOAD_SIZE + 3],
    ]) as usize;
    let end = HEADER_SIZE.saturating_add(declared).min(area.len());
    Ok(&area[..end])
}

struct FlowVerifier<'a, S: SlotStorage, C: CryptoEngine> {
    slots: &'a S,
    crypto: &'a C,
    public_key: &'a [u8; 32],
    otp_key_hash: &'a [u8; 32],
    counter: u32,
    /// header of each slot's last successful verification, by slot index
    verified: [Option<Header>; 2],
}

impl<'a, S: SlotStorage, C: CryptoEngine> SlotVerifier for FlowVerifier<'a, S, C> {
    fn verify_slot(&mut self, slot: Slot) -> Result<u32, Error> {
        let image = image_extent(self.slots.slot_bytes(slot))?;

        // XIP only: the payload must be linked for the slot it runs from
        let region = slot_region(slot);
        let regions = [Region { start: region.start + HEADER_SIZE as u32, end: region.end }];

        let policy = Policy {
            expected_type: IMAGE_TYPE_APP,
            allowed_load: &regions,
            rollback_counter: self.counter,
            public_key: self.public_key,
            otp_key_hash: self.otp_key_hash,
        };
        let h = verify_image(image, &policy, self.crypto)?;
        self.verified[slot as usize] = Some(h);
        Ok(h.security_version)
    }
}

pub fn boot_flow<R, S, F, C>(
    rot: &mut R,
    slots: &S,
    nv: &mut NvStore<F>,
    crypto: &C,
    public_key: &[u8; 32],
    cfg: &Config,
) -> Outcome
where
    R: RootOfTrust,
    S: SlotStorage,
    F: FlashMemory,
    C: CryptoEngine,
{
    if !rot.locked() {
        return Outcome::Halt(HaltCause::NotProvisioned);
    }

    // the embedded key must match the OTP hash; this is a platform fault, not an image fault, so it halts before any slot is judged and
    // before the NV state can be changed by it
    let otp_key_hash = rot.key_hash();
    if !ct_eq(&crypto.sha256(public_key), &otp_key_hash) {
        return Outcome::Halt(HaltCause::KeyHashMismatch);
    }

    let mut verifier = FlowVerifier {
        slots,
        crypto,
        public_key,
        otp_key_hash: &otp_key_hash,
        counter: rot.counter(),
        verified: [None, None],
    };
    let plan = match select_boot(nv, &mut verifier, cfg) {
        Ok(p) => p,
        Err(e) => return Outcome::Halt(HaltCause::Boot(e)),
    };

    // confirmed images raise the hardware counter; trial images do not, so they can still fall back to the previous version
    if let Some(target) = plan.counter_target {
        if target > rot.counter() {
            rot.burn_counter(target);
            if rot.counter() != target {
                return Outcome::Halt(HaltCause::CounterBurnFailed);
            }
        }
    }

    match verifier.verified[plan.slot as usize] {
        Some(h) => Outcome::Jump(JumpTarget {
            slot: plan.slot,
            mode: plan.mode,
            entry_point: h.entry_point,
        }),
        None => Outcome::Halt(HaltCause::Boot(HaltReason::NoBootableImage)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flash::FlashError;

    #[test]
    fn outcome_codes_are_distinct() {
        let jump = |mode| Outcome::Jump(JumpTarget { slot: Slot::A, mode, entry_point: 0 });
        let codes = [
            jump(BootMode::Normal).code(),
            jump(BootMode::Trial).code(),
            Outcome::Halt(HaltCause::NotProvisioned).code(),
            Outcome::Halt(HaltCause::KeyHashMismatch).code(),
            Outcome::Halt(HaltCause::Boot(HaltReason::NoBootableImage)).code(),
            Outcome::Halt(HaltCause::Boot(HaltReason::Storage(FlashError::Device))).code(),
            Outcome::Halt(HaltCause::CounterBurnFailed).code(),
        ];
        for i in 0..codes.len() {
            for j in i + 1..codes.len() {
                assert_ne!(codes[i], codes[j]);
            }
        }
    }
}
