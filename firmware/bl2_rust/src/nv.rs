//! Persistent slot state.
//!
//! Record layout (32 bytes, little-endian):
//!   0x00 u32  MAGIC 0x4E565354
//!   0x04 u16  VERSION (1)
//!   0x06 u16  reserved, zero
//!   0x08 u32  SEQ (higher = newer)
//!   0x0C u8   ACTIVE slot (0 = A, 1 = B)
//!   0x0D u8   slot A status
//!   0x0E u8   slot B status
//!   0x0F u8   TRIALS_LEFT
//!   0x10..0x1B reserved, zero
//!   0x1C u32  CRC-32 over [0x00..0x1B]
//!
//! Two sectors hold alternating copies. The newest valid record wins, and a
//! commit always erases and writes the sector that does NOT hold it.
//! (This layout will move to firmware/common/spec once the C agent uses it.)

use crate::flash::{FlashError, FlashMemory};
use crate::header::crc32;

pub const RECORD_SIZE: usize = 32;
const MAGIC: u32 = 0x4E56_5354;
const VERSION: u16 = 1;
const CRC_OFF: usize = RECORD_SIZE - 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    A = 0,
    B = 1,
}

impl Slot {
    pub const fn other(self) -> Slot {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }

    fn from_u8(v: u8) -> Option<Slot> {
        match v {
            0 => Some(Slot::A),
            1 => Some(Slot::B),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotStatus {
    Empty = 0,
    Pending = 1,
    Testing = 2,
    Confirmed = 3,
    Invalid = 4,
    Broken = 5,
}

impl SlotStatus {
    fn from_u8(v: u8) -> Option<SlotStatus> {
        match v {
            0 => Some(SlotStatus::Empty),
            1 => Some(SlotStatus::Pending),
            2 => Some(SlotStatus::Testing),
            3 => Some(SlotStatus::Confirmed),
            4 => Some(SlotStatus::Invalid),
            5 => Some(SlotStatus::Broken),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvState {
    pub seq: u32,
    /// the slot that is running (or last ran) as a confirmed image
    /// the OTA agent always writes to the other one
    pub active: Slot,
    pub slot_a: SlotStatus,
    pub slot_b: SlotStatus,
    pub trials_left: u8,
}

impl NvState {
    /// state used when no valid record exists (first boot, or both copies
    /// damaged); every boot verifies the image anyway, so this cannot be
    /// used to bypass a check
    pub const fn factory() -> NvState {
        NvState {
            seq: 0,
            active: Slot::A,
            slot_a: SlotStatus::Confirmed,
            slot_b: SlotStatus::Empty,
            trials_left: 0,
        }
    }

    pub fn status(&self, slot: Slot) -> SlotStatus {
        match slot {
            Slot::A => self.slot_a,
            Slot::B => self.slot_b,
        }
    }

    pub fn set_status(&mut self, slot: Slot, s: SlotStatus) {
        match slot {
            Slot::A => self.slot_a = s,
            Slot::B => self.slot_b = s,
        }
    }

    /// first slot (A before B) with the given status
    pub fn find(&self, s: SlotStatus) -> Option<Slot> {
        if self.slot_a == s {
            Some(Slot::A)
        } else if self.slot_b == s {
            Some(Slot::B)
        } else {
            None
        }
    }

    pub fn encode(&self) -> [u8; RECORD_SIZE] {
        let mut b = [0u8; RECORD_SIZE];
        b[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        b[4..6].copy_from_slice(&VERSION.to_le_bytes());
        b[8..12].copy_from_slice(&self.seq.to_le_bytes());
        b[12] = self.active as u8;
        b[13] = self.slot_a as u8;
        b[14] = self.slot_b as u8;
        b[15] = self.trials_left;
        let crc = crc32(&b[..CRC_OFF]);
        b[CRC_OFF..].copy_from_slice(&crc.to_le_bytes());
        b
    }

    /// none for anything that is not a fully valid record (erased flash,
    /// torn write, corruption, unknown field values)
    pub fn decode(b: &[u8; RECORD_SIZE]) -> Option<NvState> {
        if u32::from_le_bytes([b[0], b[1], b[2], b[3]]) != MAGIC {
            return None;
        }
        if u16::from_le_bytes([b[4], b[5]]) != VERSION {
            return None;
        }
        let stored = u32::from_le_bytes([b[28], b[29], b[30], b[31]]);
        if stored != crc32(&b[..CRC_OFF]) {
            return None;
        }
        if b[6] != 0 || b[7] != 0 || b[16..CRC_OFF].iter().any(|&x| x != 0) {
            return None;
        }
        Some(NvState {
            seq: u32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            active: Slot::from_u8(b[12])?,
            slot_a: SlotStatus::from_u8(b[13])?,
            slot_b: SlotStatus::from_u8(b[14])?,
            trials_left: b[15],
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateError {
    NotAllowed,
    Flash(FlashError),
}

impl From<FlashError> for UpdateError {
    fn from(e: FlashError) -> Self {
        UpdateError::Flash(e)
    }
}

pub struct NvStore<F> {
    flash: F,
    base: u32,
}

impl<F: FlashMemory> NvStore<F> {
    /// `base` is the address of the first of two consecutive NV sectors
    pub fn new(flash: F, base: u32) -> Self {
        NvStore { flash, base }
    }

    pub fn flash(&self) -> &F {
        &self.flash
    }

    pub fn into_flash(self) -> F {
        self.flash
    }

    fn sector_addr(&self, idx: u32) -> u32 {
        self.base + idx * F::SECTOR_SIZE
    }

    fn read_record(&self, idx: u32) -> Result<Option<NvState>, FlashError> {
        let mut buf = [0u8; RECORD_SIZE];
        self.flash.read(self.sector_addr(idx), &mut buf)?;
        Ok(NvState::decode(&buf))
    }

    /// (sector index, record) of the newest valid record
    fn newest(&self) -> Result<Option<(u32, NvState)>, FlashError> {
        let a = self.read_record(0)?;
        let b = self.read_record(1)?;
        Ok(match (a, b) {
            (None, None) => None,
            (Some(x), None) => Some((0, x)),
            (None, Some(y)) => Some((1, y)),
            (Some(x), Some(y)) => {
                if (y.seq.wrapping_sub(x.seq) as i32) > 0 {
                    Some((1, y))
                } else {
                    Some((0, x))
                }
            }
        })
    }

    pub fn load(&self) -> Result<Option<NvState>, FlashError> {
        Ok(self.newest()?.map(|(_, s)| s))
    }

    /// persist `st` with the next sequence number; returns what was stored,
    /// erases the sector that does not hold the newest record, so a power
    /// cut at any point leaves the previous record intact
    pub fn commit(&mut self, mut st: NvState) -> Result<NvState, FlashError> {
        let (target, seq) = match self.newest()? {
            None => (0, 1),
            Some((idx, cur)) => (1 - idx, cur.seq.wrapping_add(1)),
        };
        st.seq = seq;
        let addr = self.sector_addr(target);
        self.flash.erase_sector(addr)?;
        self.flash.write(addr, &st.encode())?;
        Ok(st)
    }

    fn current(&self) -> Result<NvState, FlashError> {
        Ok(self.load()?.unwrap_or_else(NvState::factory))
    }

    // ---- API used by the OTA agent / application -----------------------

    /// mark the passive slot as being rewritten; call before erasing it
    pub fn begin_update(&mut self, slot: Slot) -> Result<(), UpdateError> {
        let mut st = self.current()?;
        if slot == st.active || st.find(SlotStatus::Testing).is_some() {
            return Err(UpdateError::NotAllowed);
        }
        st.set_status(slot, SlotStatus::Empty);
        self.commit(st)?;
        Ok(())
    }

    /// declare the passive slot fully written (TEST_PENDING); requires a
    /// prior `begin_update` (or a never-used slot)
    pub fn mark_pending(&mut self, slot: Slot) -> Result<(), UpdateError> {
        let mut st = self.current()?;
        if slot == st.active
            || st.find(SlotStatus::Testing).is_some()
            || st.status(slot) != SlotStatus::Empty
        {
            return Err(UpdateError::NotAllowed);
        }
        st.set_status(slot, SlotStatus::Pending);
        self.commit(st)?;
        Ok(())
    }

    /// called by the running application after its self-test passes
    pub fn confirm(&mut self, slot: Slot) -> Result<(), UpdateError> {
        let mut st = self.current()?;
        if st.status(slot) != SlotStatus::Testing {
            return Err(UpdateError::NotAllowed);
        }
        st.set_status(slot, SlotStatus::Confirmed);
        st.active = slot;
        st.trials_left = 0;
        self.commit(st)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix_crc(b: &mut [u8; RECORD_SIZE]) {
        let c = crc32(&b[..CRC_OFF]);
        b[CRC_OFF..].copy_from_slice(&c.to_le_bytes());
    }

    fn sample() -> NvState {
        NvState {
            seq: 5,
            active: Slot::B,
            slot_a: SlotStatus::Confirmed,
            slot_b: SlotStatus::Testing,
            trials_left: 2,
        }
    }

    #[test]
    fn record_round_trip() {
        assert_eq!(NvState::decode(&sample().encode()), Some(sample()));
    }

    #[test]
    fn rejects_bad_crc() {
        let mut b = sample().encode();
        b[12] ^= 1;
        assert_eq!(NvState::decode(&b), None);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut b = sample().encode();
        b[0] ^= 0xFF;
        fix_crc(&mut b);
        assert_eq!(NvState::decode(&b), None);
    }

    #[test]
    fn rejects_unknown_status_value() {
        let mut b = sample().encode();
        b[13] = 9;
        fix_crc(&mut b);
        assert_eq!(NvState::decode(&b), None);
    }

    #[test]
    fn rejects_nonzero_reserved() {
        let mut b = sample().encode();
        b[20] = 1;
        fix_crc(&mut b);
        assert_eq!(NvState::decode(&b), None);
    }

    #[test]
    fn erased_flash_is_not_a_record() {
        assert_eq!(NvState::decode(&[0xFF; RECORD_SIZE]), None);
    }
}
