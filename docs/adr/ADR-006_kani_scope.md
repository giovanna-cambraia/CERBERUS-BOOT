# ADR-006: Scope and bounds of the Kani checks

- **Status:** Accepted
- **Context:** REQ-004 asks for evidence that the parsers cannot corrupt memory.
  BL1 has SPARK proofs (ADR-005). BL2 is safe Rust, with `unsafe` forbidden in
  the library, and needs evidence of its own. Kani is a bounded model checker for
  Rust: it explores all inputs within stated bounds and proves properties, or
  gives a counterexample.
- **Decision:** `firmware/bl2_rust/kani/proofs.rs` holds seven harnesses,
  run by `make kani`, for BL2's header parser, verification pipeline,
  constant-time comparison, NV record codec and slot state machine.

## What the harnesses establish

- No panic (out-of-range index, arithmetic overflow, failed unwrap) is reachable
  for any input within the bounds below.
- Functional properties: an accepted header satisfies every rule; an image is
  accepted only after a successful signature check; the check order is as
  documented; the NV codec is canonical and round-trips; `select_boot` boots only
  a slot that verified, whatever the NV flash contains.

## Bounds and modelling choices

- **Input sizes.** Header inputs of up to 260 bytes; images of up to 264 bytes (a
  payload of at most 8 bytes). The code paths do not depend on the payload size
  except through slicing, which the bound exercises, but the proof is for the
  bounded sizes.
- **Loop unwinding** is set above every loop's trip count, so Kani's unwinding
  assertions pass and the explored behaviours are complete for these sizes.
- **Crypto is modelled as an arbitrary function.** The properties therefore hold
  for any answer the real SHA-256 and Ed25519 might give. They do not say that the
  real engines are right (that is ADR-002, and the tests that go with it).
- **Flash is modelled with 64-byte sectors** in the state-machine harness instead
  of 4 KB. The NV code touches only the first 32 bytes of a sector, so the logic
  is identical, and the shorter erase loop can be unrolled.
- **Verifier answers and flash contents are arbitrary,** which includes contents
  an attacker could write.

## Not covered

- The bare-metal entry crate (`firmware/bl2_rust/entry`): it needs `unsafe` for
  memory-mapped access and is outside both Kani and the `forbid(unsafe_code)`
  library.
- The correctness of the crypto implementations, the hardware, and anything
  involving concurrency or timing.
- Kani and the CBMC version behind it are tools under active development. A run's
  claim is for the versions installed when it was made.