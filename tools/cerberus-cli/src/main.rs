use cerberus_cli::header::{self, Header};
use std::{env, fs, process::ExitCode};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn usage() -> ExitCode {
    eprintln!("usage:");
    eprintln!("  cerberus-cli example <out.bin>   write a dummy header (not signed)");
    eprintln!("  cerberus-cli dump <image.bin>    parse and print a header");
    ExitCode::from(2)
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
        signature: [0; 64], // placeholder: real signing comes in the next step
    };
    match fs::write(out, h.encode()) {
        Ok(()) => {
            println!("wrote {} bytes to {out}", header::HEADER_SIZE);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_dump(path: &str) -> ExitCode {
    let data = match fs::read(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match Header::decode(&data) {
        Ok(h) => {
            println!("image_type       0x{:04X}", h.image_type);
            println!("security_version {}", h.security_version);
            println!("payload_size     {}", h.payload_size);
            println!("load_address     0x{:08X}", h.load_address);
            println!("entry_point      0x{:08X}", h.entry_point);
            println!("nonce_salt       {}", hex(&h.nonce_salt));
            println!("image_digest     {}", hex(&h.image_digest));
            println!("signature        {}", hex(&h.signature));
            println!("[ok] structural checks passed (digest and signature not verified)");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("REJECTED: {e}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["example", out] => cmd_example(out),
        ["dump", path] => cmd_dump(path),
        _ => usage(),
    }
}
