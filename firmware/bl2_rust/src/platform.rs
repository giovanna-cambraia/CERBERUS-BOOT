//! Hardware seen from the boot flow. The target shell implements these over
//! MMIO, the tests implement them with mocks.

use crate::nv::Slot;

/// hardware root of trust (ICD-001)
pub trait RootOfTrust {
    /// ROT_STATUS bit 0: fuses are committed
    fn locked(&self) -> bool;
    /// SHA-256 of the vendor public key, from the OTP fuses
    fn key_hash(&self) -> [u8; 32];
    /// current monotonic counter value
    fn counter(&self) -> u32;
    /// request a forward burn; the caller reads the counter back to check it
    fn burn_counter(&mut self, value: u32);
}

/// memory-mapped contents of the two application slots
pub trait SlotStorage {
    /// the whole slot area, starting at the image header
    fn slot_bytes(&self, slot: Slot) -> &[u8];
}

/// key hash register mapping (ICD-001 section 3.6): word i holds digest bytes 4i..4i+3 as a little-endian value, so storing the words
// little-endian in order gives back the digest
pub fn hash_from_words(words: [u32; 8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, w) in words.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_words_are_little_endian() {
        let mut words = [0u32; 8];
        words[0] = 0x0403_0201;
        words[7] = 0xFFEE_DDCC;
        let h = hash_from_words(words);
        assert_eq!(&h[0..4], &[0x01, 0x02, 0x03, 0x04]);
        assert_eq!(&h[28..32], &[0xCC, 0xDD, 0xEE, 0xFF]);
    }
}
