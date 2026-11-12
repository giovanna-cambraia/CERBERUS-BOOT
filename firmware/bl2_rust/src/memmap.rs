//! Memory map from ASM-003. Regions are half-open: [start, end).

use crate::verify::Region;

pub const BOOT_ROM: Region = Region { start: 0x0001_0000, end: 0x0001_8000 };
pub const ROT_PERIPH: Region = Region { start: 0x0002_0000, end: 0x0002_1000 };
pub const SRAM: Region = Region { start: 0x2000_0000, end: 0x2004_0000 };
pub const BL2_FLASH: Region = Region { start: 0x4000_0000, end: 0x4002_0000 };
pub const SLOT_A: Region = Region { start: 0x4002_0000, end: 0x4010_0000 };
pub const SLOT_B: Region = Region { start: 0x4010_0000, end: 0x401E_0000 };
pub const NV_STATE: Region = Region { start: 0x401E_0000, end: 0x4020_0000 };

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::{HEADER_SIZE, MAX_PAYLOAD};

    #[test]
    fn slot_size_matches_header_spec() {
        // slot size = header + max payload, as in image_header.toml [limits]
        assert_eq!(SLOT_A.end - SLOT_A.start, MAX_PAYLOAD + HEADER_SIZE as u32);
        assert_eq!(SLOT_B.end - SLOT_B.start, MAX_PAYLOAD + HEADER_SIZE as u32);
    }
}
