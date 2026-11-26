//! OTA transport framing (firmware/common/spec/ota_protocol.md).

use crate::header::crc32;

pub const SYNC: [u8; 2] = [0xC3, 0x5A];
pub const MAX_PAYLOAD: usize = 256;
/// Image bytes per DATA frame: 4 bytes of the payload carry the offset.
pub const DATA_CHUNK: usize = MAX_PAYLOAD - 4;

pub const TYPE_BEGIN: u8 = 0x01;
pub const TYPE_DATA: u8 = 0x02;
pub const TYPE_END: u8 = 0x03;

/// One frame: sync, type, flags (0), seq, len, payload, CRC over type..payload.
pub fn encode_frame(ty: u8, seq: u16, payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() <= MAX_PAYLOAD, "payload too long for a frame");
    let mut f = Vec::with_capacity(payload.len() + 12);
    f.extend_from_slice(&SYNC);
    f.push(ty);
    f.push(0);
    f.extend_from_slice(&seq.to_le_bytes());
    f.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    f.extend_from_slice(payload);
    let crc = crc32(&f[2..]);
    f.extend_from_slice(&crc.to_le_bytes());
    f
}

/// The stream a sender produces for a whole image: BEGIN, DATA..., END.
pub fn image_to_frames(image: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut seq: u16 = 0;

    out.extend(encode_frame(TYPE_BEGIN, seq, &(image.len() as u32).to_le_bytes()));
    seq = seq.wrapping_add(1);

    for (i, chunk) in image.chunks(DATA_CHUNK).enumerate() {
        let mut payload = ((i * DATA_CHUNK) as u32).to_le_bytes().to_vec();
        payload.extend_from_slice(chunk);
        out.extend(encode_frame(TYPE_DATA, seq, &payload));
        seq = seq.wrapping_add(1);
    }

    out.extend(encode_frame(TYPE_END, seq, &crc32(image).to_le_bytes()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal decoder for the tests: (type, seq, payload) per frame.
    fn decode(stream: &[u8]) -> Vec<(u8, u16, Vec<u8>)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < stream.len() {
            assert_eq!(&stream[i..i + 2], &SYNC);
            let ty = stream[i + 2];
            assert_eq!(stream[i + 3], 0, "flags must be zero");
            let seq = u16::from_le_bytes([stream[i + 4], stream[i + 5]]);
            let len = u16::from_le_bytes([stream[i + 6], stream[i + 7]]) as usize;
            let payload = stream[i + 8..i + 8 + len].to_vec();
            let crc = u32::from_le_bytes(stream[i + 8 + len..i + 12 + len].try_into().unwrap());
            assert_eq!(crc, crc32(&stream[i + 2..i + 8 + len]));
            out.push((ty, seq, payload));
            i += 12 + len;
        }
        out
    }

    #[test]
    fn frame_layout() {
        let f = encode_frame(TYPE_BEGIN, 0x0102, &[1, 2, 3, 4]);
        assert_eq!(&f[..8], &[0xC3, 0x5A, 0x01, 0x00, 0x02, 0x01, 0x04, 0x00]);
        assert_eq!(&f[8..12], &[1, 2, 3, 4]);
        assert_eq!(f.len(), 16);
    }

    #[test]
    fn crc_covers_type_through_payload() {
        let f = encode_frame(TYPE_DATA, 9, &[0xAA; 10]);
        let crc = u32::from_le_bytes(f[f.len() - 4..].try_into().unwrap());
        assert_eq!(crc, crc32(&f[2..f.len() - 4]));
    }

    #[test]
    fn empty_payload_is_allowed() {
        assert_eq!(encode_frame(TYPE_END, 0, &[]).len(), 12);
    }

    #[test]
    #[should_panic]
    fn oversized_payload_is_refused() {
        let _ = encode_frame(TYPE_DATA, 0, &[0u8; MAX_PAYLOAD + 1]);
    }

    #[test]
    fn image_is_split_into_ordered_chunks() {
        let image: Vec<u8> = (0..600u32).map(|i| (i % 251) as u8).collect();
        let frames = decode(&image_to_frames(&image));

        assert_eq!(frames.len(), 2 + 3); // BEGIN, three DATA frames, END
        assert_eq!(frames[0].0, TYPE_BEGIN);
        assert_eq!(frames[0].2, (image.len() as u32).to_le_bytes());

        let mut rebuilt = Vec::new();
        for (n, (ty, seq, payload)) in frames[1..frames.len() - 1].iter().enumerate() {
            assert_eq!(*ty, TYPE_DATA);
            assert_eq!(*seq as usize, n + 1);
            let off = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
            assert_eq!(off, rebuilt.len());
            rebuilt.extend_from_slice(&payload[4..]);
        }
        assert_eq!(rebuilt, image);

        let (ty, _, payload) = frames.last().unwrap();
        assert_eq!(*ty, TYPE_END);
        assert_eq!(payload, &crc32(&image).to_le_bytes());
    }
}