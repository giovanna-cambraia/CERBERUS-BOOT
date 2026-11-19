#!/usr/bin/env python3
"""Ed25519 in BL1 (Monocypher, build/ed25519_check) against RFC 8032 and the
shared vectors.

The vectors are signed, and their expectations decided, by the Rust/dalek code.
Monocypher is an independent implementation, so agreement here means two
implementations accept and reject the same signatures. A vector is compared when
its expectation depends on the signature alone:
  ok, DigestMismatch        -> the signature is valid
  BadSignature              -> the signature is invalid
Vectors that fail earlier checks are skipped.
"""
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
VECTORS = ROOT / "test" / "vectors"

# RFC 8032 section 7.1, TEST 1 (empty message).
RFC_PK = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
RFC_SIG = ("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155"
           "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b")


def run(exe, *args):
    r = subprocess.run([exe, *args], capture_output=True, text=True)
    return r.stdout.strip() or "(no output)"


def change_digit(hexstr, pos=0):
    return hexstr[:pos] + ("1" if hexstr[pos] == "0" else "0") + hexstr[pos + 1:]


def main():
    exe = sys.argv[1] if len(sys.argv) > 1 else str(ROOT / "build" / "ed25519_check")
    failures = 0
    checked = 0

    def check(label, got, want):
        nonlocal failures, checked
        checked += 1
        if got == want:
            print(f"  [ok]   {label}")
        else:
            failures += 1
            print(f"  [FAIL] {label}: got {got}, want {want}")

    check("RFC 8032 test 1 verifies",
          run(exe, "raw", RFC_PK, RFC_SIG, ""), "Valid")
    check("changed message is rejected",
          run(exe, "raw", RFC_PK, RFC_SIG, "00"), "Invalid")
    check("changed signature is rejected",
          run(exe, "raw", RFC_PK, change_digit(RFC_SIG), ""), "Invalid")
    check("changed key is rejected",
          run(exe, "raw", change_digit(RFC_PK), RFC_SIG, ""), "Invalid")

    manifest = tomllib.loads((VECTORS / "manifest.toml").read_text())
    pubkey = manifest["meta"]["pubkey"]
    for v in manifest["vector"]:
        if v["expect"] in ("ok", "DigestMismatch"):
            want = "Valid"
        elif v["expect"] == "BadSignature":
            want = "Invalid"
        else:
            continue
        check(v["file"], run(exe, "image", str(VECTORS / v["file"]), pubkey), want)

    print(f"{checked} checks")
    if failures:
        print(f"[FAIL] {failures} mismatches")
        return 1
    print("[ok] Monocypher agrees with RFC 8032 and with the dalek-signed vectors")
    return 0


sys.exit(main())