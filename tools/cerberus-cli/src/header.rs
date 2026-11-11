//! Image header (HDR_VERSION 0x0001), 256 bytes, little-endian
//! Layout source of truth: firmware/common/spec/image_header.toml

use std::fmt;

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

/// signed range is [0x000..0x047]
pub const SIGNED_LEN: usize = OFF_SIG;

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, PartialEq, Eq)]
pub enum HeaderError {
    TooShort(usize),
    BadMagic(u32),
    UnsupportedVersion(u16),
    CrcMismatch { stored: u32, computed: u32 },
    ReservedNotZero,
    UnknownImageType(u16),
    BadPayloadSize(u32),
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeaderError::TooShort(n) => write!(f, "buffer too short: {n} bytes, need {HEADER_SIZE}"),
            HeaderError::BadMagic(m) => write!(f, "bad magic 0x{m:08X}"),
            HeaderError::UnsupportedVersion(v) => write!(f, "unsupported header version 0x{v:04X}"),
            HeaderError::CrcMismatch { stored, computed } => {
                write!(f, "CRC mismatch: stored 0x{stored:08X}, computed 0x{computed:08X}")
            }
            HeaderError::ReservedNotZero => write!(f, "reserved bytes are not all zero"),
            HeaderError::UnknownImageType(t) => write!(f, "unknown image type 0x{t:04X}"),
            HeaderError::BadPayloadSize(s) => write!(f, "payload size {s} out of range"),
        }
    }
}

impl std::error::Error for HeaderError {}

/// CRC-32/ISO-HDLC (same as zlib.crc32); check value for "123456789": 0xCBF43926
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

fn put_u16(b: &mut [u8], off: usize, v: u16) {
    b[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn get_u16(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}

fn get_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

impl Header {
    /// serialize to the 256-byte wire format; reserved bytes are zero and the CRC is computed over [0x000..0x0FB]
    pub fn encode(&self) -> [u8; HEADER_SIZE] {
        let mut b = [0u8; HEADER_SIZE];
        put_u32(&mut b, OFF_MAGIC, MAGIC);
        put_u16(&mut b, OFF_VERSION, HDR_VERSION);
        put_u16(&mut b, OFF_TYPE, self.image_type);
        put_u32(&mut b, OFF_SECVER, self.security_version);
        put_u32(&mut b, OFF_PAYLOAD_SIZE, self.payload_size);
        put_u32(&mut b, OFF_LOAD, self.load_address);
        put_u32(&mut b, OFF_ENTRY, self.entry_point);
        b[OFF_SALT..OFF_SALT + 16].copy_from_slice(&self.nonce_salt);
        b[OFF_DIGEST..OFF_DIGEST + 32].copy_from_slice(&self.image_digest);
        b[OFF_SIG..OFF_SIG + 64].copy_from_slice(&self.signature);
        let crc = crc32(&b[..OFF_CRC]);
        put_u32(&mut b, OFF_CRC, crc);
        b
    }

    /// the bytes the Ed25519 signature covers: header [0x000..0x047]
    pub fn signed_bytes(&self) -> [u8; SIGNED_LEN] {
        let full = self.encode();
        let mut out = [0u8; SIGNED_LEN];
        out.copy_from_slice(&full[..SIGNED_LEN]);
        out
    }

    /// parse and run the structural checks (steps 1-4 and 6 of the verifier  order), it does NOT check the digest, the signature or
    // rollback; those come later
    pub fn decode(buf: &[u8]) -> Result<Header, HeaderError> {
        if buf.len() < HEADER_SIZE {
            return Err(HeaderError::TooShort(buf.len()));
        }
        let b = &buf[..HEADER_SIZE];

        let magic = get_u32(b, OFF_MAGIC);
        if magic != MAGIC {
            return Err(HeaderError::BadMagic(magic));
        }
        let version = get_u16(b, OFF_VERSION);
        if version != HDR_VERSION {
            return Err(HeaderError::UnsupportedVersion(version));
        }
        let stored = get_u32(b, OFF_CRC);
        let computed = crc32(&b[..OFF_CRC]);
        if stored != computed {
            return Err(HeaderError::CrcMismatch { stored, computed });
        }
        if b[OFF_RESERVED..OFF_CRC].iter().any(|&x| x != 0) {
            return Err(HeaderError::ReservedNotZero);
        }
        let image_type = get_u16(b, OFF_TYPE);
        if image_type != IMAGE_TYPE_BL2 && image_type != IMAGE_TYPE_APP {
            return Err(HeaderError::UnknownImageType(image_type));
        }
        let payload_size = get_u32(b, OFF_PAYLOAD_SIZE);
        if payload_size == 0 || payload_size > MAX_PAYLOAD {
            return Err(HeaderError::BadPayloadSize(payload_size));
        }

        let mut nonce_salt = [0u8; 16];
        nonce_salt.copy_from_slice(&b[OFF_SALT..OFF_SALT + 16]);
        let mut image_digest = [0u8; 32];
        image_digest.copy_from_slice(&b[OFF_DIGEST..OFF_DIGEST + 32]);
        let mut signature = [0u8; 64];
        signature.copy_from_slice(&b[OFF_SIG..OFF_SIG + 64]);

        Ok(Header {
            image_type,
            security_version: get_u32(b, OFF_SECVER),
            payload_size,
            load_address: get_u32(b, OFF_LOAD),
            entry_point: get_u32(b, OFF_ENTRY),
            nonce_salt,
            image_digest,
            signature,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Header {
        Header {
            image_type: IMAGE_TYPE_APP,
            security_version: 7,
            payload_size: 4096,
            load_address: 0x4002_0100,
            entry_point: 0x4002_0100,
            nonce_salt: [0x11; 16],
            image_digest: [0x22; 32],
            signature: [0x33; 64],
        }
    }

    fn refresh_crc(b: &mut [u8; HEADER_SIZE]) {
        let c = crc32(&b[..OFF_CRC]);
        b[OFF_CRC..].copy_from_slice(&c.to_le_bytes());
    }

    #[test]
    fn crc_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn layout_offsets() {
        let b = sample().encode();
        assert_eq!(&b[0..4], &[0x42, 0x43, 0x45, 0x53]);
        assert_eq!(&b[4..6], &[0x01, 0x00]);
        assert_eq!(&b[6..8], &[0x02, 0x00]);
        assert_eq!(&b[8..12], &7u32.to_le_bytes());
        assert!(b[0x18..0x28].iter().all(|&x| x == 0x11));
        assert!(b[0x28..0x48].iter().all(|&x| x == 0x22));
        assert!(b[0x48..0x88].iter().all(|&x| x == 0x33));
        assert!(b[0x88..0xFC].iter().all(|&x| x == 0));
        assert_eq!(get_u32(&b, 0xFC), crc32(&b[..0xFC]));
    }

    #[test]
    fn round_trip() {
        let h = sample();
        assert_eq!(Header::decode(&h.encode()), Ok(h));
    }

    #[test]
    fn signed_bytes_are_first_72() {
        let h = sample();
        assert_eq!(h.signed_bytes().len(), 0x48);
        assert_eq!(&h.signed_bytes()[..], &h.encode()[..0x48]);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut b = sample().encode();
        b[0] ^= 0xFF;
        assert!(matches!(Header::decode(&b), Err(HeaderError::BadMagic(_))));
    }

    #[test]
    fn rejects_bad_version() {
        let mut b = sample().encode();
        b[4] = 2;
        refresh_crc(&mut b);
        assert_eq!(Header::decode(&b), Err(HeaderError::UnsupportedVersion(2)));
    }

    #[test]
    fn rejects_corrupted_byte() {
        let mut b = sample().encode();
        b[0x20] ^= 0x01;
        assert!(matches!(Header::decode(&b), Err(HeaderError::CrcMismatch { .. })));
    }

    #[test]
    fn rejects_nonzero_reserved() {
        let mut b = sample().encode();
        b[0x90] = 1;
        refresh_crc(&mut b);
        assert_eq!(Header::decode(&b), Err(HeaderError::ReservedNotZero));
    }

    #[test]
    fn rejects_unknown_image_type() {
        let mut b = sample().encode();
        b[6] = 9;
        refresh_crc(&mut b);
        assert_eq!(Header::decode(&b), Err(HeaderError::UnknownImageType(9)));
    }

    #[test]
    fn rejects_zero_payload() {
        let mut b = sample().encode();
        put_u32(&mut b, OFF_PAYLOAD_SIZE, 0);
        refresh_crc(&mut b);
        assert_eq!(Header::decode(&b), Err(HeaderError::BadPayloadSize(0)));
    }

    #[test]
    fn rejects_oversized_payload() {
        let mut b = sample().encode();
        put_u32(&mut b, OFF_PAYLOAD_SIZE, MAX_PAYLOAD + 1);
        refresh_crc(&mut b);
        assert_eq!(
            Header::decode(&b),
            Err(HeaderError::BadPayloadSize(MAX_PAYLOAD + 1))
        );
    }

    #[test]
    fn rejects_short_buffer() {
        let b = sample().encode();
        assert_eq!(Header::decode(&b[..255]), Err(HeaderError::TooShort(255)));
    }
}
