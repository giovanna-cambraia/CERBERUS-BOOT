# ADR-002: Use RustCrypto crates for SHA-256 and Ed25519 in BL2

- **Status:** Accepted
- **Context:** BL2 must hash payloads (SHA-256) and verify Ed25519
  signatures on a RISC-V RV32IMAC target with no operating system, inside a
  128 KB partition. Cryptographic code is easy to get subtly wrong.
- **Decision:** Use the `sha2` and `ed25519-dalek` crates with default
  features off (`no_std`), behind the `CryptoEngine` trait. Verify with
  `verify_strict`. Do not write our own curve arithmetic.
- **Consequences:**
  - Positive: widely used implementations, the same code runs on the host in
    tests, the trait keeps the engine replaceable.
  - Positive: with `default-features = false` the large precomputed tables
    are left out, which keeps flash use down.
  - Negative: verification is slower without those tables. To be measured
    in the Renode platform before deciding whether to turn them on.
  - Negative: third-party code in the boot path. Versions are fixed by the
    committed `Cargo.lock`, and dependencies should be audited
    (`cargo audit`) before any real use.
  - Negative: these crates are not qualified for DO-178C or similar
    standards. For a certified build the engine would be swapped through the
    `CryptoEngine` trait.
- **Alternatives considered:** a hand-written implementation (largest risk,
  least benefit); a C library such as Monocypher (smaller, but mixes the
  language of the verified core); hardware crypto (not part of this
  emulated platform).
