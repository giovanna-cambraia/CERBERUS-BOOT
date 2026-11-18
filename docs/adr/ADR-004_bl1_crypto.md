# ADR-004: Cryptography in BL1

- **Status:** Accepted
- **Context:** BL1 is a 32 KB boot ROM written in Ada/SPARK. It must verify
  BL2 before running it: SHA-256 of the payload, SHA-256 of BL2's public key
  against the OTP hash, and an Ed25519 signature. BL2 already has an
  implementation (ADR-002: `sha2` and `ed25519-dalek` in Rust).
- **Decision:**
  - SHA-256 is implemented in SPARK, with absence of run-time errors proved by
    GNATprove and behaviour checked against `hashlib` and FIPS 180-4 vectors.
  - Ed25519 verification uses Monocypher (C, public domain), vendored
    unmodified under `firmware/bl1_spark/third_party/`.
  - BL2's vendor public key sits in the first 32 bytes of the BL2 payload; the
    image's entry point is placed after it.
- **Consequences:**
  - Positive: the verified ROM stays small, and the unverified part is one
    well-known C file instead of a Rust dependency tree.
  - Positive: BL1 and BL2 use independent Ed25519 implementations, so the shared
    test vectors cross-check them.
  - Negative: the formal-verification claim covers the SPARK code (parsing,
    SHA-256, the decision), not Monocypher. It states exactly where the proof
    stops: BL1 proves it calls the primitive correctly and acts safely on the
    answer.
  - Negative: Monocypher's check is not identical to `verify_strict`: it does not
    reject small-order public keys or signature components. This does not open an
    attack here, because the key must first match the SHA-256 hash in the OTP
    fuses, so an attacker cannot choose it.
  - Negative: C inside the ROM. It's built with `-Wall -Wextra -Werror` and kept
    to this one library plus a few runtime functions.
- **Alternatives considered:** the Rust engine through a C ABI (too large for
  the ROM, large unverified surface, and no second implementation to
  cross-check); Ed25519 written in SPARK (a project of its own, far beyond the
  scope here).