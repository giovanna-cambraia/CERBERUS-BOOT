use std::path::PathBuf;
use std::{env, fs};

fn parse_hex_key(s: &str) -> Result<[u8; 32], String> {
    let s = s.trim();
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("expected 64 hex digits (a 32-byte public key)".to_string());
    }
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(key)
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Linker script.
    println!("cargo:rerun-if-changed=link.ld");
    println!("cargo:rustc-link-arg-bins=-T{}/link.ld", manifest.display());

    // vendor public key embedded in BL2: CERBERUS_VENDOR_PUB if set,
    // otherwise keys/dev.pub at the repository root
    println!("cargo:rerun-if-env-changed=CERBERUS_VENDOR_PUB");
    let (path, explicit) = match env::var("CERBERUS_VENDOR_PUB") {
        Ok(p) => (PathBuf::from(p), true),
        Err(_) => (manifest.join("../../../keys/dev.pub"), false),
    };
    println!("cargo:rerun-if-changed={}", path.display());

    let key = match fs::read_to_string(&path) {
        Ok(text) => parse_hex_key(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
        Err(e) if explicit => panic!("CERBERUS_VENDOR_PUB={}: {e}", path.display()),
        Err(_) => {
            println!(
                "cargo:warning=no vendor key at {}: embedding an all-zero key, BL2 will halt (run `make sign-keys`)",
                path.display()
            );
            [0u8; 32]
        }
    };

    let body: Vec<String> = key.iter().map(|b| format!("0x{b:02X}")).collect();
    let src = format!(
        "#[link_section = \".vendor_key\"]\n#[used]\npub static VENDOR_PUBKEY: [u8; 32] = [{}];\n",
        body.join(", ")
    );
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("vendor_key.rs");
    fs::write(out, src).unwrap();
}