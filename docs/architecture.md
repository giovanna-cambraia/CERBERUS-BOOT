# Architecture

## Memory map (ASM-003)

| Region | Address | Size | Contents |
|---|---|---|---|
| Boot ROM | `0x0001_0000` | 32 KB | BL1 |
| RoT peripheral | `0x0002_0000` | 4 KB | ICD-001 registers |
| SRAM | `0x2000_0000` | 256 KB | BSS, stacks, reports |
| BL2 partition | `0x4000_0000` | 128 KB | 256-byte header, then payload |
| Slot A | `0x4002_0000` | 896 KB | application image (header + payload) |
| Slot B | `0x4010_0000` | 896 KB | application image |
| NV state | `0x401E_0000` | 128 KB | two 4 KB sectors used |

SRAM conventions: BL1's stack tops out at `0x2003_A000`; BL1's boot report is at
`0x2003_B000`; BL2's stack tops out at `0x2004_0000` and keeps at least 16 KB
below it. The test application writes its marker at `0x2003_0000`.

Images are linked to run in place from the slot they sit in. An image linked for
slot A cannot run from slot B.

## Boot sequence

1. Reset. The hart starts at the BL1 entry in the boot ROM.
2. BL1 runs the checks below. On success it raises the BL2 counter and jumps to
   BL2's entry point; on any failure it records the reason and halts.
3. BL2 starts at `_start`, which sits directly after the vendor key (the first
   32 bytes of its payload). It checks that the RoT is provisioned and that its
   embedded key hashes to the OTP value, then lets the slot state machine pick an
   image to verify.
4. For a confirmed image, BL2 raises the application counter and jumps. For a
   new image it boots on trial (3 boots by default) until the application calls
   `confirm`.

## Verification order

BL1, on the BL2 image (expected type: BL2):

| # | Check | Failure code |
|---|---|---|
| 1 | Header: magic, version, CRC, reserved bytes, type, payload size | 1 `Header_Invalid` |
| 2 | Placement: load address is the payload start, payload fits the partition, entry point inside | 2 `Placement_Invalid` |
| 3 | RoT fuses are committed | 3 `Not_Provisioned` |
| 4 | BL2 version is not below the BL2 counter | 4 `Rollback_Rejected` |
| 5 | SHA-256 of the 32-byte key equals the OTP hash | 5 `Key_Hash_Mismatch` |
| 6 | SHA-256 of the payload, streamed from flash, equals the header digest | 6 `Digest_Mismatch` |
| 7 | Ed25519 signature over header bytes 0 to 71 | 7 `Signature_Invalid` |
| 8 | After all passed: the BL2 counter reads back as raised | 8 `Counter_Burn_Failed` |

BL2, on an application slot (expected type: application): RoT provisioned and
key hash first (a platform fault that must not touch slot state), then for the
chosen slot: header checks, exact image size, placement inside the slot, version
against the application counter, payload digest, signature. A slot that fails is
marked `Invalid` and the other one is tried.

Cheap checks run before expensive ones, and the streamed payload hash only runs
for an image whose key is already trusted. The header's version is checked before
the signature, which can only make the boot more restrictive.

## Image format

256-byte header, little-endian, followed by the payload. The signature covers
header bytes 0 to 71, which include the digest of the payload, so the payload is
bound to the signature through it. Specification: `firmware/common/spec/image_header.md`
(machine-readable: `firmware/common/spec/image_header.toml`).

A BL2 payload starts with the 32-byte vendor public key; its entry point follows
the key (ADR-004).

## Slot state

A 32-byte record with a sequence number and a CRC, stored alternately in two
sectors so that a power cut at any byte leaves the previous record valid.
Statuses: Empty, Pending, Testing, Confirmed, Invalid, Broken. Specification and
operation table: `firmware/common/spec/nv_state.md`.

## Root of trust (ICD-001)

| Offset | Register | Access | Notes |
|---|---|---|---|
| `0x000` | ROT_STATUS | R | bit 0 locked, bit 1 illegal access (sticky until reset) |
| `0x004` to `0x020` | ROT_PUBKEY_HASH_0..7 | R | digest bytes 4i..4i+3, little-endian |
| `0x024` | ROT_MONOTONIC_CTR | R/W | application counter (BL2) |
| `0x028` | ROT_LIFECYCLE_CMD | W | `0xA5A50001` commits the fuses |
| `0x02C` | ROT_BL2_CTR | R/W | BL2 counter (BL1) |

Counters only move forward. Full interface: `docs/interfaces/ICD-001_rot_peripheral.md`.

## Trust anchors and keys

- The only trust anchor is the SHA-256 of the vendor public key in the OTP fuses.
- BL2 carries the key itself (first 32 bytes of its payload); BL1 hashes it and
  compares with the OTP value before using it.
- `keys/dev.key` and `keys/dev.pub` are development keys, generated per machine
  and never committed. The test vectors use a fixed, public test key, documented
  as such in `test/vectors/README.md`.

## Debug reports

BL1 writes seven words at `0x2003_B000`: magic `0xB101_0002`, parse status,
failure code, decision (0 jump, 1 halt), entry point, BL2 version, BL2 counter.

BL2 writes four words to its `BOOT_REPORT` symbol: magic `0xB001_0001`, code,
slot (0 = A, 1 = B, `0xFF` none), entry. Codes: 0 normal boot, 1 trial boot,
`0x10` not provisioned, `0x11` key hash mismatch, `0x12` no bootable image,
`0x13` storage error, `0x14` counter burn failed.

## Toolchains

| Component | Toolchain |
|---|---|
| BL1 (target) | `riscv64-elf` GNAT with a runtime derived at build time, `-march=rv32imac -mabi=ilp32` |
| BL1 (host tools, proofs) | Ubuntu GNAT, GNATprove from Alire |
| BL2 | `rustc`, target `riscv32imac-unknown-none-elf` |
| Host tool | `rustc`, host target |
| Root of trust | GHDL, VHDL-2008 |
| Emulator | Renode |

Build scripts: `scripts/build_bl1_rv32.sh` and `scripts/make_rv32_rts.sh`.