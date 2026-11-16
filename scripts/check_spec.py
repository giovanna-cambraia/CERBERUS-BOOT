#!/usr/bin/env python3
"""Validate firmware/common/spec/image_header.toml."""
import sys
import tomllib
import zlib

PATH = sys.argv[1] if len(sys.argv) > 1 else "firmware/common/spec/image_header.toml"

with open(PATH, "rb") as f:
    spec = tomllib.load(f)

errors = []
fields = spec["field"]
total = spec["header"]["size"]
names = {f["name"]: f for f in fields}

pos = 0
for f in fields:
    if f["offset"] != pos:
        errors.append(f"{f['name']}: offset 0x{f['offset']:03X}, expected 0x{pos:03X}")
    if f["size"] in (2, 4) and f["offset"] % f["size"]:
        errors.append(f"{f['name']}: not naturally aligned")
    pos = f["offset"] + f["size"]
if pos != total:
    errors.append(f"fields end at {pos}, header size is {total}")

s_lo, s_hi = spec["signing"]["covers"]
sig = names[spec["signing"]["field"]]
if s_lo != 0:
    errors.append("signed range must start at 0x000")
if s_hi + 1 != sig["offset"]:
    errors.append(f"signed range ends 0x{s_hi:03X}, SIGNATURE starts 0x{sig['offset']:03X}")
for need in ("NONCE_SALT", "IMAGE_DIGEST"):
    n = names[need]
    if not (s_lo <= n["offset"] and n["offset"] + n["size"] - 1 <= s_hi):
        errors.append(f"{need} is not inside the signed range")

crc = names[spec["crc"]["field"]]
c_lo, c_hi = spec["crc"]["covers"]
if crc["offset"] + crc["size"] != total:
    errors.append("CRC field must be last")
if c_lo != 0 or c_hi + 1 != crc["offset"]:
    errors.append("CRC must cover [0 .. crc offset - 1]")

got = zlib.crc32(spec["crc"]["check_string"].encode()) & 0xFFFFFFFF
if got != spec["crc"]["check_value"]:
    errors.append(f"CRC check value mismatch: 0x{got:08X}")
magic = int.from_bytes(bytes(spec["header"]["magic_bytes"]), "little")
if magic != spec["header"]["magic_value"]:
    errors.append(f"magic_bytes decode to 0x{magic:08X}, not magic_value")

lim = spec["limits"]
if lim["max_payload"] != lim["slot_size"] - total:
    errors.append("max_payload != slot_size - header size")

if errors:
    print("SPEC ERRORS:")
    for e in errors:
        print("  -", e)
    sys.exit(1)

print(f"{'Offset':<8}{'Size':<6}Field")
for f in fields:
    print(f"0x{f['offset']:03X}   {f['size']:<6}{f['name']}")
print(f"\n[ok] {len(fields)} fields, {total} bytes, signed range "
      f"0x{s_lo:03X}..0x{s_hi:03X}, CRC check value OK")
