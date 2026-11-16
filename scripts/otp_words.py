#!/usr/bin/env python3
"""OTP hash words for a vendor public key.

  python3 scripts/otp_words.py [keys/dev.pub] [--bin build/bl2.bin]

Prints the SHA-256 of the key and the eight ICD-001 register words (word i =
digest bytes 4i..4i+3, little-endian) as VHDL literals. With --bin, also checks
that the key is embedded in the given BL2 binary.
"""
import hashlib
import struct
import sys
from pathlib import Path


def main():
    args = sys.argv[1:]
    binpath = None
    if "--bin" in args:
        i = args.index("--bin")
        binpath = args[i + 1]
        del args[i:i + 2]

    keyfile = Path(args[0] if args else "keys/dev.pub")
    try:
        key = bytes.fromhex(keyfile.read_text().strip())
    except (OSError, ValueError) as e:
        sys.exit(f"{keyfile}: {e}")
    if len(key) != 32:
        sys.exit(f"{keyfile}: expected 32 bytes, got {len(key)}")

    digest = hashlib.sha256(key).digest()
    print("public key   ", key.hex())
    print("sha256(key)  ", digest.hex())
    print("OTP words (ICD-001, little-endian):")
    for i, w in enumerate(struct.unpack("<8I", digest)):
        print(f'  ROT_PUBKEY_HASH_{i}  0x{w:08X}   x"{w:08X}"')

    if binpath:
        data = Path(binpath).read_bytes()
        pos = data.find(key)
        if pos < 0:
            print(f"[FAIL] key not found in {binpath}")
            return 1
        print(f"[ok] key embedded in {binpath} at payload offset 0x{pos:X}")
    return 0


sys.exit(main())
