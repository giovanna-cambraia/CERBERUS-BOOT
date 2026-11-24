# Threat model (STRIDE)

Scope: the boot chain from the boot ROM to the application, and the update
path as far as it is implemented. Out of scope, as in the specification:
fault injection (laser, glitching) and key-hierarchy features such as
revocation. Severity is my assessment for this design.

Evidence names refer to `docs/verification/renode-scenarios.md` and to the
commands in `docs/verification/VTP-001.md`.

| ID | Class | Threat | Sev. | Mitigation built | Evidence | Residual risk |
|---|---|---|---|---|---|---|
| T-01 | Spoofing | A rogue BL2 signed with the attacker's own key | High | BL2's key must hash to the OTP value, then the signature must verify | chain-attacker-key | OEM key compromise: the key must live in an offline HSM; development keys are in use here |
| T-02 | Spoofing | A rogue or forged application delivered by update | High | BL2 verifies key hash, digest and signature before any slot can run | bad-update; `make test-bl2` (vectors `bad_signature_*`, `forged_*`) | Same key-compromise risk |
| T-03 | Spoofing | A device with unprovisioned or mismatching fuses | Med | BL1 and BL2 refuse to boot without a committed OTP matching BL2's key | chain-unprovisioned, chain-wrong-otp, unprovisioned, wrong-otp | None beyond fuse programming errors |
| T-04 | Tampering | Payload modified in flash or in transit | High | SHA-256 of the payload is bound by the signature | chain-tampered-payload, tampered; vector `bad_digest` | None known |
| T-05 | Tampering | Header edited, then its CRC fixed (version lowered, load address moved) | High | The signature covers bytes 0 to 71, so the CRC is not a security control | vectors `forged_secver`, `forged_load_address`; chain-bad-signature | None known |
| T-06 | Tampering | The slot-state record in NV flash is modified | Med | None cryptographic; every boot verifies the image it selects, so the record cannot cause an unverified boot | tampered, rollback; `docs` note in `firmware/common/spec/nv_state.md` | The record is unauthenticated: an attacker with flash access can cause denial of service, or boot an older image that is validly signed and not below the counter |
| T-07 | Tampering | Software with bus access abuses the RoT registers (burns a counter to the maximum) | Med | Counters are forward-only; the lifecycle lock is one-way | `make test-hdl` (T4 to T7, T12 to T14) | No privilege separation on the emulated bus: a malicious application can still deny service by burning counters |
| T-08 | Repudiation | Denying that an update was deployed | Low | Signed headers bind digest, version and image type | `make test-cli` | No on-device audit log |
| T-09 | Information disclosure | Firmware or fuse contents read from the device | Low | None: the code is not secret, and the key hash is public by design | none | Accepted; optional image encryption from the specification is not implemented |
| T-10 | Denial of service | Power loss during an update | High | Alternating NV records, an A/B scheme, and trial boots with fallback | `make test-bl2` power-loss sweeps; fallback, update | Flash wear: no write-rate limiter; no injection in the emulator |
| T-11 | Denial of service | Malformed or oversized header fields | Med | Bounded parsing; BL1's parser is proved free of run-time errors | `make prove`; vectors `bad_payload_size_*`, `bad_header_too_short`; chain-bad-header | The C update parser is not written (REQ-005) |
| T-12 | Elevation of privilege | Rolling back to an older, validly signed image | Critical | One monotonic counter per stage, checked before the signature | rollback, chain-rollback | A BL2 that verifies but is faulty cannot be rolled back (no BL2 A/B) |
| T-13 | Elevation of privilege | Ratcheting the counters with a forged high version | Med | Counters are raised only after the whole verification passed | chain scenarios where verification fails leave the counters unchanged | None known |
| T-14 | Elevation of privilege | A bug that makes BL1 jump to unverified code | Critical | The jump is reachable only through a function whose postcondition is proved | `make prove` | Unproved parts: Monocypher, the hardware-access code, the assembly, the compiler (ADR-005) |
| T-15 | Elevation of privilege | Substituting a different BL2 key while keeping a valid header | High | The key is part of the digest-protected payload and is checked against the OTP hash | chain-attacker-key | None known |
| T-16 | Information disclosure | Timing differences in digest comparison | Low | Constant-time style comparison in BL1 (`Equal`) and BL2 | `make test-sha256` (functional only) | The compiler could reintroduce early exits; the compared values are public |
| T-17 | Tampering | Compromised build dependencies | Med | Lockfiles are committed; the vendored library's version and tarball checksum are recorded | `firmware/bl1_spark/third_party/monocypher/VENDORED.md` | No automated audit of Rust crates yet |
| T-18 | Elevation of privilege | Fault injection (glitching the verification result) | High | None | none | Out of scope |