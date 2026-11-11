//! Minimal hex helpers (keys are stored as hex text)

pub fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return Err("odd number of hex digits".to_string());
    }
    if !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("non-hex character".to_string());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| format!("bad hex: {e}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let data = [0u8, 1, 254, 255];
        assert_eq!(from_hex(&to_hex(&data)), Ok(data.to_vec()));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(from_hex("abc").is_err());
        assert!(from_hex("zz").is_err());
        assert!(from_hex("+f").is_err());
    }
}
