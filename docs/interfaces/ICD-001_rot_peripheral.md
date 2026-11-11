# ICD-001: Hardware Root-of-Trust Interface

Implementation: `hdl/src/rot_peripheral.vhd`
Verification: `hdl/tb/rot_peripheral_tb.vhd` (`make test-hdl`)

## 1. Address window

Base address `0x0002_0000`, 4 KB window (ASM-003). All offsets below are
relative to the base. Registers are 32-bit and word-aligned.

## 2. Register map

| Offset        | Name                 | Access | Description |
|---------------|----------------------|--------|-------------|
| 0x000         | ROT_STATUS           | R      | bit 0: LOCKED (1 = fuses committed). bit 1: ERROR (sticky illegal-access flag). bits 31..2 read as 0. |
| 0x004..0x020  | ROT_PUBKEY_HASH_0..7 | R      | SHA-256 digest of the vendor public key, word 0 at 0x004 through word 7 at 0x020. |
| 0x024         | ROT_MONOTONIC_CTR    | R/W    | Anti-rollback counter. A write burns it forward only (see 3.2). |
| 0x028         | ROT_LIFECYCLE_CMD    | W      | Write `0xA5A50001` to commit (lock) the fuses. Reads as 0. |
| other         | (unmapped)           |        | Reads return 0. Writes set ROT_STATUS.ERROR. |

## 3. Behavior

### 3.1 State classes

- **Non-volatile:** LOCKED, the counter, and the key hash. They are not
  affected by `rst_n`. A simulator restart models a power cycle.
- **Volatile:** the ERROR flag. It is cleared by `rst_n`.

### 3.2 Monotonic counter

A write is accepted only if the written value is **strictly greater** than
the current value. Otherwise the counter is unchanged and ERROR is set.
This covers decrements and equal-value writes. The maximum value is
`0xFFFFFFFF`; after that every write is rejected.

### 3.3 Lifecycle command

Writing `0xA5A50001` sets LOCKED permanently. Writing it again is accepted
with no effect. Any other value leaves LOCKED unchanged and sets ERROR.

### 3.4 Illegal accesses

These set ERROR and change no state: any write to ROT_STATUS or
ROT_PUBKEY_HASH_*, any write to an unmapped offset, a rejected counter write,
and a lifecycle write with the wrong value. ERROR stays set until reset.

### 3.5 Provisioning model (simulation)

The key hash is fixed at elaboration (VHDL generic `PUBKEY_HASH`) and is
readable before and after the commit. LOCKED indicates whether the device is
committed. Per-word fuse programming before the commit is not modeled yet.

## 4. Bus protocol

Synchronous, single-cycle, no wait states.

| Signal      | Dir | Width | Description |
|-------------|-----|-------|-------------|
| clk         | in  | 1     | Bus clock. |
| rst_n       | in  | 1     | Active-low reset (volatile state only). |
| bus_sel     | in  | 1     | Access strobe. |
| bus_wr      | in  | 1     | 1 = write, 0 = read. |
| bus_addr    | in  | 12    | Byte offset into the window. |
| bus_wdata   | in  | 32    | Write data, sampled on the rising clock edge. |
| bus_rdata   | out | 32    | Read data, combinational from `bus_addr` while `bus_sel=1` and `bus_wr=0`. Otherwise 0. |

## 5. Test traceability

| Test | Behavior checked |
|------|------------------|
| T1-T3  | Reset state: status 0, hash words, counter 0 |
| T4     | Counter burns forward |
| T5, T7 | Decrement and equal-value writes rejected, ERROR set |
| T6     | Reset clears ERROR, keeps the counter |
| T8     | Hash register is read-only |
| T9     | Bad lifecycle value does not lock |
| T10-T11| Commit locks, and the lock survives reset |
| T12    | Counter maximum and rejection after it |
