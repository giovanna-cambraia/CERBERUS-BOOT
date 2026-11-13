# Slot State Record Specification (version 1)

The slot-state record tells BL2 which of the two application slots to boot
and how far a new image has got through its trial. BL2 and the OTA agent
both read and write it, so they must follow this document exactly.
Machine-readable source of truth: `nv_state.toml`.

## Storage

- NV region: `0x401E_0000`, 128 KB (ASM-003). Only the first two erase
  sectors are used; the rest is reserved.
- Sector size is 4096 bytes (emulated flash). Sector 0 is at `0x401E_0000`,
  sector 1 at `0x401E_1000`.
- Each sector holds one record, starting at the sector's first byte.
- Erased flash reads `0xFF`, and a program operation can only clear bits.

## Record layout (32 bytes, little-endian)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0x00 | 4 | MAGIC | value `0x4E565354`, stored bytes `54 53 56 4E` |
| 0x04 | 2 | VERSION | `1` |
| 0x06 | 2 | RESERVED0 | zero |
| 0x08 | 4 | SEQ | sequence number, higher is newer |
| 0x0C | 1 | ACTIVE | `0` = A, `1` = B |
| 0x0D | 1 | SLOT_A_STATUS | status code |
| 0x0E | 1 | SLOT_B_STATUS | status code |
| 0x0F | 1 | TRIALS_LEFT | boots left for a slot in `Testing` |
| 0x10 | 12 | RESERVED1 | zero |
| 0x1C | 4 | CRC32 | CRC-32/ISO-HDLC over `[0x00..0x1B]` |

## A record is valid only if all of these hold

1. MAGIC and VERSION match.
2. CRC32 matches bytes `[0x00..0x1B]`.
3. RESERVED0 and RESERVED1 are all zero.
4. ACTIVE is 0 or 1, and both statuses are defined codes.

Anything else (erased flash, a torn write, corruption) is "no record".

## Reading

1. Read the record at the start of each sector and discard invalid ones.
2. If both are valid, the newer one wins: record X is newer than Y when
   `(X.SEQ - Y.SEQ)` modulo 2^32, read as a signed 32-bit number, is
   greater than zero. This stays correct across wraparound.
3. If neither is valid, use the **factory state**: ACTIVE = A, slot A
   `Confirmed`, slot B `Empty`, TRIALS_LEFT = 0.

## Writing (a commit)

A writer MUST do exactly this, so a power cut at any point keeps the
previous record usable:

1. Find the newest valid record. Its sector is the *current* sector.
2. Choose the **other** sector as the target (sector 0 if there is no
   valid record). The new SEQ is the newest SEQ + 1 (1 if none).
3. Erase the target sector.
4. Program the new 32-byte record into it.

Never erase or modify the current sector. Only one writer may run at a
time: BL2 runs before the application starts, and the application's OTA
agent must be single-threaded with respect to NV writes.

## Slot statuses

| Code | Name | Meaning |
|---|---|---|
| 0 | Empty | No usable image, or being rewritten |
| 1 | Pending | Fully written by the OTA agent, not yet verified |
| 2 | Testing | Verified, booting on trial, not yet confirmed |
| 3 | Confirmed | Confirmed good |
| 4 | Invalid | Failed verification or the rollback gate |
| 5 | Broken | Used up its trial boots without being confirmed |

ACTIVE is the slot that is running (or last ran) as a confirmed image. The
OTA agent always writes to the other slot.

## Operations

| Operation | Who | Precondition | Effect |
|---|---|---|---|
| begin_update(slot) | OTA agent | slot is not ACTIVE; no slot is `Testing` | slot becomes `Empty` |
| mark_pending(slot) | OTA agent | same, and slot is `Empty` | slot becomes `Pending` |
| confirm(slot) | application | slot is `Testing` | slot becomes `Confirmed`, ACTIVE = slot, TRIALS_LEFT = 0 |
| boot: `Pending` slot verifies | BL2 | | `Testing`, TRIALS_LEFT = N - 1 (this boot is one trial) |
| boot: `Pending` slot fails | BL2 | | `Invalid` |
| boot: `Testing`, TRIALS_LEFT > 0, verifies | BL2 | | TRIALS_LEFT decreases by 1 |
| boot: `Testing`, TRIALS_LEFT = 0 | BL2 | | `Broken`, then fall back |
| boot: `Testing` fails verification | BL2 | | `Invalid`, TRIALS_LEFT = 0, then fall back |
| boot: `Confirmed` fails verification | BL2 | | `Invalid`, try the other slot |
| boot: other `Confirmed` slot is used | BL2 | | ACTIVE = that slot |

N is the number of trial boots, 3 by default (`policy.default_trial_boots`).

The OTA agent's sequence is: `begin_update`, erase and write the image into
the passive slot, `mark_pending`, reset. The application's sequence after
a trial boot is: run self-test, then `confirm`.

BL2 verifies the signature, digest and rollback gate of a slot every time it
boots it, whatever the record says. The record only decides *which* slot to
try.

## Security notes

- The record is **not authenticated**. Someone who can write the NV region
  can change which slot is tried, or force a reset to the factory state. Image
  verification (signature, digest, rollback counter) is independent of it, so
  the worst outcomes are a denial of service or booting an older image that is
  still validly signed and not below the rollback counter.
- After a confirmed update, the hardware counter is burned to the new
  version on the next boot, which blocks the old slot. A trial image can
  always fall back, because the counter is only burned for confirmed images.

## Power-loss guarantee

If power is lost at any byte of a commit, the previous record is still
valid. This is tested in `firmware/bl2_rust/tests/state_machine.rs`.
