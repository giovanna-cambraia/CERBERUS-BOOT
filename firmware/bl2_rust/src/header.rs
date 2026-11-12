//! Image header parsing (HDR_VERSION 0x0001), structural checks only.
//! Layout source of truth: firmware/common/spec/image_header.toml

use crate::error::Error;

pub const HEADER_SIZE: usize = 256;
pub const MAGIC: u32 = 0x5345_4342; // wire bytes: 42 43 45 53
pub const HDR_VERSION: u16 = 0x0001;
pub const IMAGE_TYPE_BL2: u16 = 0x0001;
pub const IMAGE_TYPE_APP: u16 = 0x0002;
pub const MAX_PAYLOAD: u32 = 0x000D_FF00; // slot 0xE0000 - header 0x100

pub const OFF_MAGIC: usize = 0x000;
pub const OFF_VERSION: usize = 0x004;
pub const OFF_TYPE: usize = 0x006;
pub const OFF_SECVER: usize = 0x008;
pub const OFF_PAYLOAD_SIZE: usize = 0x00C;
pub const OFF_LOAD: usize = 0x010;
pub const OFF_ENTRY: usize = 0x014;
pub const OFF_SALT: usize = 0x018;
pub const OFF_DIGEST: usize = 0x028;
pub const OFF_SIG: usize = 0x048;
pub const OFF_RESERVED: usize = 0x088;
pub const OFF_CRC: usize = 0x0FC;

/// the signature covers header bytes [0x000..0x047]
pub const SIGNED_LEN: usize = OFF_SIG;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub image_type: u16,
    pub security_version: u32,
    pub payload_size: u32,
    pub load_address: u32,
    pub entry_point: u32,
    pub nonce_salt: [u8; 16],
    pub image_digest: [u8; 32],
    pub signature: [u8; 64],
}

/// CRC-32/ISO-HDLC. Check value for "123456789": 0xCBF43926
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn u16_at(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn array_at<const N: usize>(b: &[u8], off: usize) -> [u8; N] {
    let mut out = [0u8; N];
    out.copy_from_slice(&b[off..off + N]);
    out
}

impl Header {
    /// verifier steps 1-6, in order: magic, version, CRC, reserved bytes, image type (known, then expected for this slot), payload size;
    /// digest, signature, load region and rollback are checked in `verify`
    pub fn parse(buf: &[u8], expected_type: u16) -> Result<Header, Error> {
        if buf.len() < HEADER_SIZE {
            return Err(Error::TooShort);
        }
        let b = &buf[..HEADER_SIZE];

        if u32_at(b, OFF_MAGIC) != MAGIC {
            return Err(Error::BadMagic);
        }
        if u16_at(b, OFF_VERSION) != HDR_VERSION {
            return Err(Error::UnsupportedVersion);
        }
        if u32_at(b, OFF_CRC) != crc32(&b[..OFF_CRC]) {
            return Err(Error::CrcMismatch);
        }
        if b[OFF_RESERVED..OFF_CRC].iter().any(|&x| x != 0) {
            return Err(Error::ReservedNotZero);
        }
        let image_type = u16_at(b, OFF_TYPE);
        if image_type != IMAGE_TYPE_BL2 && image_type != IMAGE_TYPE_APP {
            return Err(Error::UnknownImageType);
        }
        if image_type != expected_type {
            return Err(Error::WrongImageType);
        }
        let payload_size = u32_at(b, OFF_PAYLOAD_SIZE);
        if payload_size == 0 || payload_size > MAX_PAYLOAD {
            return Err(Error::BadPayloadSize);
        }

        Ok(Header {
            image_type,
            security_version: u32_at(b, OFF_SECVER),
            payload_size,
            load_address: u32_at(b, OFF_LOAD),
            entry_point: u32_at(b, OFF_ENTRY),
            nonce_salt: array_at(b, OFF_SALT),
            image_digest: array_at(b, OFF_DIGEST),
            signature: array_at(b, OFF_SIG),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix_crc(b: &mut [u8; HEADER_SIZE]) {
        let c = crc32(&b[..OFF_CRC]);
        b[OFF_CRC..].copy_from_slice(&c.to_le_bytes());
    }

    /// a structurally valid header (digest and signature are not checked here)
    fn raw() -> [u8; HEADER_SIZE] {
        let mut b = [0u8; HEADER_SIZE];
        b[OFF_MAGIC..OFF_MAGIC + 4].copy_from_slice(&MAGIC.to_le_bytes());
        b[OFF_VERSION..OFF_VERSION + 2].copy_from_slice(&HDR_VERSION.to_le_bytes());
        b[OFF_TYPE..OFF_TYPE + 2].copy_from_slice(&IMAGE_TYPE_APP.to_le_bytes());
        b[OFF_PAYLOAD_SIZE..OFF_PAYLOAD_SIZE + 4].copy_from_slice(&64u32.to_le_bytes());
        fix_crc(&mut b);
        b
    }

    #[test]
    fn crc_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn parse_accepts_valid_header() {
        let h = Header::parse(&raw(), IMAGE_TYPE_APP).unwrap();
        assert_eq!(h.payload_size, 64);
        assert_eq!(h.image_type, IMAGE_TYPE_APP);
    }

    #[test]
    fn rejects_too_short() {
        let b = raw();
        assert_eq!(Header::parse(&b[..255], IMAGE_TYPE_APP).err(), Some(Error::TooShort));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut b = raw();
        b[0] ^= 0xFF;
        assert_eq!(Header::parse(&b, IMAGE_TYPE_APP).err(), Some(Error::BadMagic));
    }

    #[test]
    fn rejects_bad_version() {
        let mut b = raw();
        b[OFF_VERSION] = 2;
        fix_crc(&mut b);
        assert_eq!(Header::parse(&b, IMAGE_TYPE_APP).err(), Some(Error::UnsupportedVersion));
    }

    #[test]
    fn rejects_bad_crc() {
        let mut b = raw();
        b[OFF_SALT] ^= 1;
        assert_eq!(Header::parse(&b, IMAGE_TYPE_APP).err(), Some(Error::CrcMismatch));
    }

    #[test]
    fn rejects_nonzero_reserved() {
        let mut b = raw();
        b[OFF_RESERVED + 8] = 1;
        fix_crc(&mut b);
        assert_eq!(Header::parse(&b, IMAGE_TYPE_APP).err(), Some(Error::ReservedNotZero));
    }

    #[test]
    fn rejects_unknown_type() {
        let mut b = raw();
        b[OFF_TYPE] = 9;
        fix_crc(&mut b);
        assert_eq!(Header::parse(&b, IMAGE_TYPE_APP).err(), Some(Error::UnknownImageType));
    }

    #[test]
    fn rejects_wrong_type_for_slot() {
        assert_eq!(Header::parse(&raw(), IMAGE_TYPE_BL2).err(), Some(Error::WrongImageType));
    }

    #[test]
    fn rejects_zero_payload() {
        let mut b = raw();
        b[OFF_PAYLOAD_SIZE..OFF_PAYLOAD_SIZE + 4].copy_from_slice(&0u32.to_le_bytes());
        fix_crc(&mut b);
        assert_eq!(Header::parse(&b, IMAGE_TYPE_APP).err(), Some(Error::BadPayloadSize));
    }

    #[test]
    fn rejects_oversized_payload() {
        let mut b = raw();
        b[OFF_PAYLOAD_SIZE..OFF_PAYLOAD_SIZE + 4]
            .copy_from_slice(&(MAX_PAYLOAD + 1).to_le_bytes());
        fix_crc(&mut b);
        assert_eq!(Header::parse(&b, IMAGE_TYPE_APP).err(), Some(Error::BadPayloadSize));
    }
}
