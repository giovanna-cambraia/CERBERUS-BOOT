//! Flash abstraction (DIP: boot logic depends on this trait, not on MMIO).
//! Semantics follow NOR flash: an erased byte reads 0xFF, and a write can
//! only clear bits (new = old AND data).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashError {
    OutOfRange,
    Device,
}

pub trait FlashMemory {
    /// erase granularity in bytes
    const SECTOR_SIZE: u32;

    fn read(&self, addr: u32, buf: &mut [u8]) -> Result<(), FlashError>;

    /// erase the sector starting at `addr` (must be sector-aligned)
    fn erase_sector(&mut self, addr: u32) -> Result<(), FlashError>;

    /// program bytes into erased flash
    fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), FlashError>;
}
