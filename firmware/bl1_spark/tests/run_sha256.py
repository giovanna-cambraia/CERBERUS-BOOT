#!/usr/bin/env python3
"""SHA-256 in SPARK (build/sha256_check) against hashlib and FIPS 180-4.

Cases: the standard known-answer messages, every length from 0 to 299 bytes
(which covers all the padding boundaries, including 55/56 and 63/64), one
million 'a's, and every image in test/vectors.
"""
import hashlib
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]

KNOWN = {  # FIPS 180-4 / NIST examples
    "empty": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "abc": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    "448 bits": "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    "million a": "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
}


def main():
    exe = sys.argv[1] if len(sys.argv) > 1 else str(ROOT / "build" / "sha256_check")

    cases = [
        ("empty", b""),
        ("abc", b"abc"),
        ("448 bits", b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        ("million a", b"a" * 1_000_000),
    ]
    for n in range(300):
        cases.append((f"{n} bytes", bytes((i * 7 + n) & 0xFF for i in range(n))))
    for p in sorted((ROOT / "test" / "vectors").glob("*.img")):
        cases.append((p.name, p.read_bytes()))

    failures = 0
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "input.bin"
        for label, data in cases:
            path.write_bytes(data)
            out = subprocess.run([exe, str(path)], capture_output=True, text=True)
            got = out.stdout.strip()
            want = hashlib.sha256(data).hexdigest()
            if got != want or (label in KNOWN and got != KNOWN[label]):
                failures += 1
                print(f"  [FAIL] {label}: got {got or '(nothing)'}, want {want}")

    print(f"{len(cases)} inputs checked")
    if failures:
        print(f"[FAIL] {failures} mismatches")
        return 1
    print("[ok] SHA-256 in SPARK agrees with hashlib and FIPS 180-4")
    return 0


sys.exit(main())