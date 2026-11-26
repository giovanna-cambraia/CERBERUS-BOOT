#!/usr/bin/env python3
"""Validate firmware/common/spec/nv_state.toml.

Checks the layout, runs a reference-codec self-test, and compares the spec
with the Rust and C sources so they cannot drift apart.

  python3 scripts/check_nv_spec.py            validate
  python3 scripts/check_nv_spec.py --example  also print the example record (hex)
  python3 scripts/check_nv_spec.py --rust     print the example record as a Rust array
  python3 scripts/check_nv_spec.py --c        print the example record as a C array
"""
import re
import struct
import sys
import tomllib
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SPEC = ROOT / "firmware/common/spec/nv_state.toml"
NV_RS = ROOT / "firmware/bl2_rust/src/nv.rs"
MEMMAP_RS = ROOT / "firmware/bl2_rust/src/memmap.rs"
BOOT_RS = ROOT / "firmware/bl2_rust/src/boot.rs"
TEST_RS = ROOT / "firmware/bl2_rust/tests/state_machine.rs"
C_NV_H = ROOT / "firmware/app_c/include/cerberus_nv.h"

# same example as `sample()` in the unit tests of nv.rs.
EXAMPLE = {"seq": 5, "active": 1, "slot_a": 3, "slot_b": 2, "trials_left": 2}


def hexint(s):
    return int(s.replace("_", ""), 16)


# ---- reference codec ---------------------------------------------------------

def fields_by_name(spec):
    return {f["name"]: f for f in spec["field"]}


def crc_of(spec, buf):
    crc_off = fields_by_name(spec)["CRC32"]["offset"]
    return zlib.crc32(bytes(buf[:crc_off])) & 0xFFFFFFFF


def fix_crc(spec, buf):
    off = fields_by_name(spec)["CRC32"]["offset"]
    struct.pack_into("<I", buf, off, crc_of(spec, buf))


def encode(spec, st):
    f = fields_by_name(spec)
    buf = bytearray(spec["record"]["size"])
    struct.pack_into("<I", buf, f["MAGIC"]["offset"], spec["record"]["magic_value"])
    struct.pack_into("<H", buf, f["VERSION"]["offset"], spec["record"]["version"])
    struct.pack_into("<I", buf, f["SEQ"]["offset"], st["seq"])
    buf[f["ACTIVE"]["offset"]] = st["active"]
    buf[f["SLOT_A_STATUS"]["offset"]] = st["slot_a"]
    buf[f["SLOT_B_STATUS"]["offset"]] = st["slot_b"]
    buf[f["TRIALS_LEFT"]["offset"]] = st["trials_left"]
    fix_crc(spec, buf)
    return bytes(buf)


def decode(spec, raw):
    """Return the state dict, or None if the record is not valid."""
    f = fields_by_name(spec)
    if len(raw) != spec["record"]["size"]:
        return None
    if struct.unpack_from("<I", raw, f["MAGIC"]["offset"])[0] != spec["record"]["magic_value"]:
        return None
    if struct.unpack_from("<H", raw, f["VERSION"]["offset"])[0] != spec["record"]["version"]:
        return None
    if struct.unpack_from("<I", raw, f["CRC32"]["offset"])[0] != crc_of(spec, raw):
        return None
    for name, fld in f.items():
        if name.startswith("RESERVED") and any(raw[fld["offset"]:fld["offset"] + fld["size"]]):
            return None
    st = {
        "seq": struct.unpack_from("<I", raw, f["SEQ"]["offset"])[0],
        "active": raw[f["ACTIVE"]["offset"]],
        "slot_a": raw[f["SLOT_A_STATUS"]["offset"]],
        "slot_b": raw[f["SLOT_B_STATUS"]["offset"]],
        "trials_left": raw[f["TRIALS_LEFT"]["offset"]],
    }
    if st["active"] not in spec["slot"].values():
        return None
    valid_status = set(spec["status"].values())
    if st["slot_a"] not in valid_status or st["slot_b"] not in valid_status:
        return None
    return st


def newer(a_seq, b_seq):
    """True if b is newer than a (wrapping signed comparison)."""
    d = (b_seq - a_seq) & 0xFFFFFFFF
    return 0 < d < 0x80000000


# ---- checks ------------------------------------------------------------------

def check_layout(spec, errors):
    fields = spec["field"]
    size = spec["record"]["size"]
    names = fields_by_name(spec)
    for need in ("MAGIC", "VERSION", "SEQ", "ACTIVE", "SLOT_A_STATUS",
                 "SLOT_B_STATUS", "TRIALS_LEFT", "CRC32"):
        if need not in names:
            errors.append(f"missing field {need}")
            return

    pos = 0
    for f in fields:
        if f["offset"] != pos:
            errors.append(f"{f['name']}: offset 0x{f['offset']:02X}, expected 0x{pos:02X}")
        if f["size"] in (2, 4) and f["offset"] % f["size"]:
            errors.append(f"{f['name']}: not naturally aligned")
        pos = f["offset"] + f["size"]
    if pos != size:
        errors.append(f"fields end at {pos}, record size is {size}")

    crc = names[spec["crc"]["field"]]
    lo, hi = spec["crc"]["covers"]
    if crc["offset"] + crc["size"] != size:
        errors.append("CRC field must be last")
    if lo != 0 or hi + 1 != crc["offset"]:
        errors.append("CRC must cover [0 .. crc offset - 1]")
    got = zlib.crc32(spec["crc"]["check_string"].encode()) & 0xFFFFFFFF
    if got != spec["crc"]["check_value"]:
        errors.append(f"CRC check value mismatch: 0x{got:08X}")

    magic = int.from_bytes(bytes(spec["record"]["magic_bytes"]), "little")
    if magic != spec["record"]["magic_value"]:
        errors.append(f"magic_bytes decode to 0x{magic:08X}, not magic_value")


def check_storage(spec, errors):
    s = spec["storage"]
    size = spec["record"]["size"]
    if s["base"] % s["sector_size"]:
        errors.append("storage.base is not sector-aligned")
    if s["sectors"] * s["sector_size"] > s["region_size"]:
        errors.append("sectors do not fit in region_size")
    if s["record_offset"] + size > s["sector_size"]:
        errors.append("record does not fit in a sector")
    for table in ("slot", "status"):
        vals = list(spec[table].values())
        if len(set(vals)) != len(vals):
            errors.append(f"[{table}] has duplicate values")
        if any(not 0 <= v <= 255 for v in vals):
            errors.append(f"[{table}] has a value outside 0..255")


def find(pattern, text, what, errors, flags=0):
    m = re.search(pattern, text, flags)
    if not m:
        errors.append(f"cannot find {what} in the Rust sources")
    return m


def enum_values(text, name, errors):
    m = find(rf"pub enum {name}\s*\{{(.*?)\n\}}", text, f"enum {name}", errors, re.S)
    if not m:
        return None
    return {k: int(v) for k, v in re.findall(r"(\w+)\s*=\s*(\d+)", m.group(1))}


def check_rust(spec, errors):
    for p in (NV_RS, MEMMAP_RS, BOOT_RS, TEST_RS):
        if not p.exists():
            errors.append(f"missing {p.relative_to(ROOT)}")
            return
    nv, mm = NV_RS.read_text(), MEMMAP_RS.read_text()
    boot, tst = BOOT_RS.read_text(), TEST_RS.read_text()
    rec, st = spec["record"], spec["storage"]

    m = find(r"const MAGIC: u32 = (0x[0-9A-Fa-f_]+);", nv, "MAGIC", errors)
    if m and hexint(m.group(1)) != rec["magic_value"]:
        errors.append(f"nv.rs MAGIC {m.group(1)} != spec 0x{rec['magic_value']:08X}")

    m = find(r"const VERSION: u16 = (\d+);", nv, "VERSION", errors)
    if m and int(m.group(1)) != rec["version"]:
        errors.append(f"nv.rs VERSION {m.group(1)} != spec {rec['version']}")

    m = find(r"pub const RECORD_SIZE: usize = (\d+);", nv, "RECORD_SIZE", errors)
    if m and int(m.group(1)) != rec["size"]:
        errors.append(f"nv.rs RECORD_SIZE {m.group(1)} != spec {rec['size']}")

    for enum_name, table in (("Slot", "slot"), ("SlotStatus", "status")):
        got = enum_values(nv, enum_name, errors)
        if got is not None and got != spec[table]:
            errors.append(f"nv.rs enum {enum_name} {got} != spec [{table}] {spec[table]}")

    m = find(r"pub const NV_STATE: Region = Region \{ start: (0x[0-9A-Fa-f_]+), "
             r"end: (0x[0-9A-Fa-f_]+) \};", mm, "NV_STATE", errors)
    if m:
        start, end = hexint(m.group(1)), hexint(m.group(2))
        if start != st["base"] or end != st["base"] + st["region_size"]:
            errors.append(f"memmap.rs NV_STATE 0x{start:08X}..0x{end:08X} != spec "
                          f"0x{st['base']:08X}..0x{st['base'] + st['region_size']:08X}")

    m = find(r"Config \{ trial_boots: (\d+) \}", boot, "default trial_boots", errors)
    if m and int(m.group(1)) != spec["policy"]["default_trial_boots"]:
        errors.append(f"boot.rs default trial_boots {m.group(1)} != spec "
                      f"{spec['policy']['default_trial_boots']}")

    m = find(r"const SECTOR: usize = (\d+);", tst, "mock SECTOR", errors)
    if m and int(m.group(1)) != st["sector_size"]:
        errors.append(f"state_machine.rs SECTOR {m.group(1)} != spec sector_size {st['sector_size']}")


def check_c(spec, errors):
    if not C_NV_H.exists():
        errors.append(f"missing {C_NV_H.relative_to(ROOT)}")
        return
    text = C_NV_H.read_text()
    rec, st = spec["record"], spec["storage"]

    def number(name):
        m = re.search(rf"#define\s+{name}\s+(0x[0-9A-Fa-f]+|\d+)u", text)
        if not m:
            errors.append(f"cannot find {name} in cerberus_nv.h")
            return None
        return int(m.group(1), 0)

    for name, want in (("NV_MAGIC", rec["magic_value"]),
                       ("NV_VERSION", rec["version"]),
                       ("NV_RECORD_SIZE", rec["size"]),
                       ("NV_BASE_ADDR", st["base"]),
                       ("NV_SECTOR_SIZE", st["sector_size"]),
                       ("NV_SECTOR_COUNT", st["sectors"]),
                       ("NV_DEFAULT_TRIAL_BOOTS", spec["policy"]["default_trial_boots"])):
        got = number(name)
        if got is not None and got != want:
            errors.append(f"cerberus_nv.h {name} {got} != spec {want}")

    for prefix, table in (("NV_SLOT_", "slot"), ("NV_STATUS_", "status")):
        got = {k.lower(): int(v) for k, v in re.findall(rf"{prefix}(\w+)\s*=\s*(\d+)", text)}
        want = {k.lower(): v for k, v in spec[table].items()}
        if got != want:
            errors.append(f"cerberus_nv.h {prefix}* {got} != spec [{table}] {want}")


def selftest(spec, errors):
    raw = encode(spec, EXAMPLE)
    if decode(spec, raw) != EXAMPLE:
        errors.append("example record does not round-trip")
        return 0

    accepted = 0
    for bit in range(len(raw) * 8):
        m = bytearray(raw)
        m[bit // 8] ^= 1 << (bit % 8)
        if decode(spec, bytes(m)) is not None:
            accepted += 1
    if accepted:
        errors.append(f"{accepted} single-bit corruptions were accepted")

    size = spec["record"]["size"]
    storage_erased = bytes([spec["storage"]["erased_byte"]]) * size
    if decode(spec, storage_erased) is not None:
        errors.append("erased flash decoded as a record")
    if decode(spec, bytes(size)) is not None:
        errors.append("all-zero bytes decoded as a record")

    f = fields_by_name(spec)
    bad_status = bytearray(raw)
    bad_status[f["SLOT_A_STATUS"]["offset"]] = 0x7F
    fix_crc(spec, bad_status)
    if decode(spec, bytes(bad_status)) is not None:
        errors.append("unknown status value was accepted")

    bad_reserved = bytearray(raw)
    bad_reserved[f["RESERVED1"]["offset"] + 3] = 1
    fix_crc(spec, bad_reserved)
    if decode(spec, bytes(bad_reserved)) is not None:
        errors.append("non-zero reserved bytes were accepted")

    if not newer(0xFFFFFFFF, 0) or newer(0, 0xFFFFFFFF) or newer(5, 5) or not newer(5, 6):
        errors.append("sequence comparison is wrong")
    return len(raw) * 8


def main():
    with open(SPEC, "rb") as fh:
        spec = tomllib.load(fh)

    if "--c" in sys.argv:
        raw = encode(spec, EXAMPLE)
        print("/* Generated by scripts/check_nv_spec.py --c from nv_state.toml. */")
        print("#include <stdint.h>")
        print("static const uint8_t GOLDEN_NV_RECORD[32] = {")
        for i in range(0, len(raw), 8):
            print("    " + ", ".join(f"0x{b:02X}" for b in raw[i:i + 8]) + ",")
        print("};")
        return 0

    errors = []
    check_layout(spec, errors)
    if not errors:
        check_storage(spec, errors)
        check_rust(spec, errors)
        check_c(spec, errors)
        flips = selftest(spec, errors)

    if errors:
        print("NV SPEC ERRORS:")
        for e in errors:
            print("  -", e)
        return 1

    print(f"{'Offset':<8}{'Size':<6}Field")
    for f in spec["field"]:
        print(f"0x{f['offset']:02X}    {f['size']:<6}{f['name']}")
    print(f"\n[ok] {len(spec['field'])} fields, {spec['record']['size']}-byte record, "
          f"{flips} single-bit corruptions rejected, Rust and C sources match the spec")

    raw = encode(spec, EXAMPLE)
    if "--example" in sys.argv:
        print("example record:", raw.hex())
    if "--rust" in sys.argv:
        rows = [", ".join(f"0x{b:02X}" for b in raw[i:i + 8]) for i in range(0, len(raw), 8)]
        print("const GOLDEN: [u8; RECORD_SIZE] = [")
        for r in rows:
            print(f"    {r},")
        print("];")
    return 0


if __name__ == "__main__":
    sys.exit(main())