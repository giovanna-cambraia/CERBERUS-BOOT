# Requirements traceability

This maps the five requirements of the project specification to the design and
to the evidence that exists in the repository. It is a statement of what is
checked, not a certification claim. Standard references are the ones named in
the specification.

Status: **Covered** (design and evidence exist), **Partial** (some of it is
missing), **Open** (not started).

Evidence is produced by the commands named in each row. `make verify` runs them
all; `make docs-check` confirms that every command, file and scenario named
here exists.

## Requirements

| ID | Requirement | Standard reference | Status |
|---|---|---|---|
| REQ-001 | Integrity check of the second-stage boot code before it runs | NIST SP 800-193 4.2.1 | Covered |
| REQ-002 | Prevention of firmware downgrade | IEC 62443-4-2 CR 7.6; ISO 21434 10.4.1 | Covered |
| REQ-003 | Dual-slot update that cannot brick the device | NIST SP 800-193 4.4.1; ISO 26262-6 7.4.3 | Covered |
| REQ-004 | Freedom from memory corruption in the parsers | DO-178C Table A-5; ISO 26262-6 Table 1 | Covered |
| REQ-005 | No unbounded buffer overflows in the update-path parser | MISRA C:2012 Dir 4.11, Rules 17.7, 21.8; CERT C ARR30-C | Partial |

## REQ-001: integrity of BL2

- **Design:** `firmware/bl1_spark/src/bl1-boot.adb` runs the checks listed in
  `docs/architecture.md`; `firmware/bl1_spark/src/bl1-policy.ads` allows the jump
  only when every one passed.
- **Evidence:**
  - `make prove`: no run-time errors in BL1's own code, and the proved properties
    "jump if and only if all checks passed" (`Decide`) and "no jump unless no
    failure is recorded" (`Check_BL2`).
  - `make test-bl1`, `make test-sha256`, `make test-ed25519`: BL1's parser,
    SHA-256 and Ed25519 against the shared vectors, hashlib, FIPS 180-4 and RFC 8032.
  - `make renode-bl1`: the same SPARK source compiled for RV32 gives the same
    verdicts as the host build, on every vector.
  - `make chain-all` (and with `ROT=ghdl`): chain-ok, chain-tampered-payload,
    chain-bad-signature, chain-bad-header, chain-attacker-key, chain-wrong-type,
    chain-wrong-otp, chain-unprovisioned.
- **Gaps:** the boot ROM is not write-protected in the emulator, so "immutable"
  is assumed from the platform, not demonstrated. Monocypher, the
  hardware-access code and the assembly are outside the proof (ADR-005).

## REQ-002: anti-rollback

- **Design:** one counter per stage (`docs/interfaces/ICD-001_rot_peripheral.md`,
  section 3.7). BL2 checks application versions (`firmware/bl2_rust/src/verify.rs`);
  BL1 checks BL2's version. The VHDL accepts a counter write only if it is larger
  than the current value (`hdl/src/rot_peripheral.vhd`).
- **Evidence:**
  - `make test-hdl` (T4 to T7, T12 to T14) and `make bridge-test`: counters only
    move forward and are independent.
  - `make test-bl2`: `rollback_counter_is_enforced` and
    `rollback_counter_blocks_an_old_image`, and the `forged_secver` vector, which
    has a valid header checksum but a lowered version.
  - `make renode-all`: rollback. `make chain-all`: chain-rollback, chain-bl2-upgrade.
- **Gaps:** BL1's `Counter_Burn_Failed` path (the counter does not read back as
  raised) is covered by the proof but has no emulator scenario; BL2's equivalent
  has the unit test `counter_burn_failure_halts`.

## REQ-003: resilient updates

- **Design:** alternating NV records, trial boots and fallback
  (`firmware/bl2_rust/src/nv.rs`, `firmware/bl2_rust/src/boot.rs`; format in
  `firmware/common/spec/nv_state.md`).
- **Evidence:**
  - `make test-bl2`, in `firmware/bl2_rust/tests/state_machine.rs`:
    `power_loss_during_commit_never_loses_state` (a power cut at every byte of
    every commit, across both sectors), `power_loss_during_boot_selection_still_boots`,
    `trial_boots_are_limited`, `trial_image_that_stops_verifying_falls_back`,
    `update_happy_path`, `update_api_guards`.
  - `make renode-all`: update, bad-update, fallback, tampered.
  - `make spec-check`: the NV format document, its TOML, the Rust constants and a
    golden record agree.
  - `make update-all` (and with `ROT=ghdl`): update-ok, update-corrupt,
    update-bad-signature, update-failed-selftest. A real application in C stages
    an update, BL2 trial-boots it, the application confirms itself or fails and is
    rolled back, through resets, on the emulated platform.
- **Gaps:** power loss is injected in unit tests only, not in the emulator. The
  transport is a memory-mapped mailbox standing in for a UART, with a preloaded
  stream and no retransmission.

## REQ-004: parsers free of memory-safety errors

- **Design:** BL1's header parser, CRC, SHA-256 and boot logic are SPARK; BL2's
  parser, verification pipeline, slot state machine and NV codec are safe Rust
  (`#![forbid(unsafe_code)]` in `firmware/bl2_rust/src/lib.rs`).
- **Evidence:**
  - `make prove`: absence of index, range and overflow errors in the SPARK units.
  - `make kani`: bounded model checking of the Rust side, in
    `firmware/bl2_rust/kani/proofs.rs`, for every input within the bounds of
    ADR-006: `verify_header_parsing` (no panic, and an accepted header satisfies
    every rule), `verify_check_order`, `verify_image_accepts_only_verified_images`
    (an image is accepted only after a successful signature check, whatever the
    crypto returns), `verify_ct_eq_is_equality`, `verify_nv_decode_is_canonical`,
    `verify_nv_round_trip`, `verify_select_boot_only_boots_verified_slots` (for any
    NV contents, only a slot that verified is booted).
  - The vector suites run on BL1 and BL2 with the same files (`make test-bl1`,
    `make renode-bl1`, `make test-bl2`).
- **Gaps:** Kani's results hold within its bounds (ADR-006): images up to 264
  bytes, 64-byte NV sectors, crypto modelled as arbitrary. The hardware-access
  code (BL2's `entry` crate, which needs `unsafe`) is outside both tools.

## REQ-005: the C update parser

- **Design:** `firmware/app_c/src/ota_parser.c` (byte-at-a-time state machine, fixed
  buffers, no heap, no recursion), `firmware/app_c/src/ota_agent.c` and
  `firmware/app_c/src/cerberus_nv.c`; protocol in
  `firmware/common/spec/ota_protocol.md`.
- **Evidence:**
  - `make test-app`: parser behaviour (garbage, damaged frames, oversized and
    malformed headers, every single-bit corruption of a frame), agent refusals,
    the slot-state writer against the golden record generated from the spec, a
    power cut at every byte of a commit, and the Rust frame encoder against the C
    one. The production sources compile with `-Wconversion -Wsign-conversion
    -Wshadow -Werror` and more.
  - `make fuzz-app`: mutated frame streams through the parser and agent under
    ASan and UBSan, with an invariant that the active slot is never written.
  - `make app`: the same sources compile freestanding for RV32.
  - `make spec-check`: the C constants match the NV specification.
  - `make update-all`: the agent running on the emulated hart, fed the
    Rust-framed stream, including a damaged one.
- **Gaps:** `make lint-app` (cppcheck's MISRA C:2012 addon) is wired but its
  findings are not triaged, so MISRA compliance is not claimed; there is no
  AFL++ or libFuzzer campaign, only the built-in mutator.

## Properties beyond the five requirements

| Property | Evidence |
|---|---|
| The shared formats cannot drift apart between codebases | `make spec-check`; the vector manifests checked by `make test-cli`, `make test-bl2` and `make test-bl1` |
| Two independent Ed25519 implementations agree | `make test-ed25519` (Monocypher, BL1) against vectors signed by the Rust engine (dalek, BL2) and RFC 8032; `make test-bl2` (`engine_agrees_with_every_vector`) |
| The RoT behaves as its interface document says | `make test-hdl`, `make bridge-test`, and the full scenario sets with `ROT=ghdl` (`make cosim-all`, `make chain-all ROT=ghdl`) |
| A platform fault does not corrupt slot state | `make test-bl2` (`key_hash_mismatch_halts_and_leaves_nv_alone`); `make renode-all` (wrong-otp, unprovisioned) |
| The test vectors reproduce byte for byte | `make vectors-check` |