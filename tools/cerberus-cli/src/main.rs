use cerberus_cli::header::{self, Header};
use cerberus_cli::hexutil::{from_hex, to_hex};
use cerberus_cli::image::{build_image, sha256, verify_image, ImageParams};
use cerberus_cli::vectors;
use ed25519_dalek::SigningKey;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!("usage:");
    eprintln!("  cerberus-cli keygen <basename>        writes <basename>.key and <basename>.pub");
    eprintln!("  cerberus-cli sign --key K --payload P --out O --type app|bl2 --secver N");
    eprintln!("                    [--load ADDR] [--entry ADDR]");
    eprintln!("  cerberus-cli verify --pub K.pub --image I");
    eprintln!("  cerberus-cli dump <image>             print the header (structural checks only)");
    eprintln!("  cerberus-cli example <out.bin>        dummy unsigned header");
    ExitCode::from(2)
    eprintln!("  cerberus-cli vectors <outdir>         write deterministic test vectors + manifest");
}

fn flag<'a>(args: &[&'a str], name: &str) -> Option<&'a str> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1])
}

fn req<'a>(args: &[&'a str], name: &str) -> Result<&'a str, String> {
    flag(args, name).ok_or(format!("missing {name}"))
}

fn parse_u32(s: &str) -> Result<u32, String> {
    let s = s.replace('_', "");
    let r = match s.strip_prefix("0x") {
        Some(h) => u32::from_str_radix(h, 16),
        None => s.parse::<u32>(),
    };
    r.map_err(|e| format!("bad number '{s}': {e}"))
}

fn read_key32(path: &str) -> Result<[u8; 32], String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let bytes = from_hex(&text).map_err(|e| format!("{path}: {e}"))?;
    bytes
        .try_into()
        .map_err(|_| format!("{path}: expected 32 bytes of hex"))
}

fn write_new(path: &str, contents: &str, secret: bool) -> std::io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true); // never overwrite an existing key
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        if secret {
            opts.mode(0o600);
        }
    }
    #[cfg(not(unix))]
    let _ = secret;
    opts.open(path)?.write_all(contents.as_bytes())
}

fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {msg}");
    ExitCode::FAILURE
}

fn print_header(h: &Header) {
    println!("image_type       0x{:04X}", h.image_type);
    println!("security_version {}", h.security_version);
    println!("payload_size     {}", h.payload_size);
    println!("load_address     0x{:08X}", h.load_address);
    println!("entry_point      0x{:08X}", h.entry_point);
    println!("nonce_salt       {}", to_hex(&h.nonce_salt));
    println!("image_digest     {}", to_hex(&h.image_digest));
    println!("signature        {}", to_hex(&h.signature));
}

fn cmd_keygen(base: &str) -> ExitCode {
    let mut seed = [0u8; 32];
    if let Err(e) = getrandom::getrandom(&mut seed) {
        return fail(format!("random source: {e}"));
    }
    let sk = SigningKey::from_bytes(&seed);
    let pk = sk.verifying_key().to_bytes();

    let key_path = format!("{base}.key");
    let pub_path = format!("{base}.pub");
    if let Err(e) = write_new(&key_path, &format!("{}\n", to_hex(&seed)), true) {
        return fail(format!("{key_path}: {e}"));
    }
    if let Err(e) = write_new(&pub_path, &format!("{}\n", to_hex(&pk)), false) {
        return fail(format!("{pub_path}: {e}"));
    }
    println!("wrote {key_path} (private, DEV ONLY) and {pub_path}");
    println!("public key        {}", to_hex(&pk));
    println!("sha256(public key) {}   <- value for the OTP hash", to_hex(&sha256(&pk)));
    ExitCode::SUCCESS
}

fn sign_inner(a: &[&str]) -> Result<String, String> {
    let key_path = req(a, "--key")?;
    let payload_path = req(a, "--payload")?;
    let out_path = req(a, "--out")?;
    let secver = req(a, "--secver")?
        .parse::<u32>()
        .map_err(|e| format!("--secver: {e}"))?;

    let (image_type, default_load) = match req(a, "--type")? {
        "app" => (header::IMAGE_TYPE_APP, 0x4002_0100u32), // slot A + header
        "bl2" => (header::IMAGE_TYPE_BL2, 0x4000_0100u32), // BL2 partition + header
        other => return Err(format!("--type must be app or bl2, got '{other}'")),
    };
    let load = match flag(a, "--load") {
        Some(s) => parse_u32(s)?,
        None => default_load,
    };
    let entry = match flag(a, "--entry") {
        Some(s) => parse_u32(s)?,
        None => load,
    };

    let sk = SigningKey::from_bytes(&read_key32(key_path)?);
    let payload = fs::read(payload_path).map_err(|e| format!("{payload_path}: {e}"))?;
    let mut salt = [0u8; 16];
    getrandom::getrandom(&mut salt).map_err(|e| format!("random source: {e}"))?;

    let params = ImageParams {
        image_type,
        security_version: secver,
        load_address: load,
        entry_point: entry,
        nonce_salt: salt,
    };
    let image = build_image(&params, &payload, &sk).map_err(|e| e.to_string())?;
    fs::write(out_path, &image).map_err(|e| format!("{out_path}: {e}"))?;
    Ok(format!(
        "wrote {out_path}: {} bytes (header 256 + payload {}), load 0x{load:08X}, secver {secver}",
        image.len(),
        payload.len()
    ))
}

fn cmd_sign(a: &[&str]) -> ExitCode {
    match sign_inner(a) {
        Ok(msg) => {
            println!("{msg}");
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn cmd_verify(a: &[&str]) -> ExitCode {
    let (pub_path, img_path) = match (req(a, "--pub"), req(a, "--image")) {
        (Ok(p), Ok(i)) => (p, i),
        (Err(e), _) | (_, Err(e)) => return fail(e),
    };
    let pk = match read_key32(pub_path) {
        Ok(k) => k,
        Err(e) => return fail(e),
    };
    let image = match fs::read(img_path) {
        Ok(d) => d,
        Err(e) => return fail(format!("{img_path}: {e}")),
    };
    match verify_image(&pk, &image) {
        Ok(h) => {
            print_header(&h);
            println!("[ok] structure, size, SHA-256 digest and Ed25519 signature verified");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("REJECTED: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_dump(path: &str) -> ExitCode {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => return fail(format!("{path}: {e}")),
    };
    match Header::decode(&data) {
        Ok(h) => {
            print_header(&h);
            println!("[ok] structural checks passed (digest and signature NOT verified)");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("REJECTED: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_example(out: &str) -> ExitCode {
    let h = Header {
        image_type: header::IMAGE_TYPE_APP,
        security_version: 1,
        payload_size: 4096,
        load_address: 0x4002_0100,
        entry_point: 0x4002_0100,
        nonce_salt: [0x11; 16],
        image_digest: [0x22; 32],
        signature: [0; 64],
    };
    match fs::write(out, h.encode()) {
        Ok(()) => {
            println!("wrote {} bytes to {out}", header::HEADER_SIZE);
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let cmd = args.first().copied().unwrap_or("");
    let rest = &args[args.len().min(1)..];
    match (cmd, rest) {
        ("keygen", [base]) => cmd_keygen(base),
        ("sign", r) => cmd_sign(r),
        ("verify", r) => cmd_verify(r),
        ("dump", [path]) => cmd_dump(path),
        ("example", [out]) => cmd_example(out),
        ("vectors", [dir]) => cmd_vectors(dir),
        _ => usage(),
    }
}

fn cmd_vectors(dir: &str) -> ExitCode {
    if let Err(e) = fs::create_dir_all(dir) {
        return fail(format!("{dir}: {e}"));
    }
    let vs = vectors::generate();
    for v in &vs {
        let path = format!("{dir}/{}", v.file);
        if let Err(e) = fs::write(&path, &v.bytes) {
            return fail(format!("{path}: {e}"));
        }
    }
    let mpath = format!("{dir}/manifest.toml");
    if let Err(e) = fs::write(&mpath, vectors::manifest(&vs)) {
        return fail(format!("{mpath}: {e}"));
    }
    println!("wrote {} vectors and manifest.toml to {dir}", vs.len());
    println!("test public key {}", to_hex(&vectors::test_public_key()));
    ExitCode::SUCCESS
}
