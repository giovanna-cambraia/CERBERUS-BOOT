#!/usr/bin/env python3
"""Run the BL1 header parser (build/bl1_check) over test/vectors.

The parser only decides structural checks (steps 1-6 of the verifier order),
so for each vector in manifest.toml it expects:
  - the manifest's own error, if the vector fails at step 1..6 or is too short,
  - "Ok" for everything that fails later (digest, signature) and for valid images,
  - nothing for SizeMismatch (needs the payload length, which BL1 reads later).
"""
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
VECTORS = ROOT / "test" / "vectors"


def main():
    exe = sys.argv[1] if len(sys.argv) > 1 else str(ROOT / "build" / "bl1_check")
    manifest = tomllib.loads((VECTORS / "manifest.toml").read_text())

    checked = skipped = failed = 0
    for v in manifest["vector"]:
        name, expect, step = v["file"], v["expect"], v.get("step")
        if expect == "SizeMismatch":
            skipped += 1
            continue
        kind = "bl2" if name.startswith("valid_bl2") else "app"
        out = subprocess.run([exe, str(VECTORS / name), kind],
                             capture_output=True, text=True)
        got = out.stdout.splitlines()[0] if out.stdout.strip() else "(no output)"

        if expect == "TooShort" or (step is not None and 1 <= step <= 6):
            want = expect
        else:
            want = "Ok"

        checked += 1
        if got == want:
            print(f"  [ok]   {name:<34} {got}")
        else:
            failed += 1
            print(f"  [FAIL] {name:<34} got {got}, want {want}")

    print(f"{checked} vectors checked, {skipped} skipped (need the payload length)")
    if failed:
        print(f"[FAIL] {failed} mismatches")
        return 1
    print("[ok] BL1 header parser agrees with the shared vectors")
    return 0


sys.exit(main())