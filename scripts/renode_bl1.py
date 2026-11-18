#!/usr/bin/env python3
"""Run the BL1 boot ROM, built for RV32, in Renode against the shared vectors.

For every vector the first 256 bytes are placed at the start of the BL2
partition, the hart is reset, and BL1 runs. The report BL1 writes to SRAM is
compared with what the host build of the same SPARK code says about the same
file (build/bl1_check <file> bl2): same source, same answer on x86 and RV32.

  python3 scripts/renode_bl1.py [--renode renode]
"""
import argparse
import json
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "build" / "renode_bl1"
VECTORS = ROOT / "test" / "vectors"

BL1_BASE = 0x0001_0000
BL2_BASE = 0x4000_0000
REPORT = 0x2003_B000
REPORT_MAGIC = 0xB101_0001
SAFE_HALT = 1  # Decision'Pos (Safe_Halt): without crypto BL1 never jumps

# Parse_Status'Pos in firmware/bl1_spark/src/bl1-header.ads
STATUS_CODE = {
    "Ok": 0, "BadMagic": 1, "UnsupportedVersion": 2, "CrcMismatch": 3,
    "ReservedNotZero": 4, "UnknownImageType": 5, "WrongImageType": 6,
    "BadPayloadSize": 7,
}

ROT_STUB = """\
# Stub: BL1 does not use the RoT yet.
if request.isRead:
    request.value = 0
"""


def host_status(tool, path):
    r = subprocess.run([tool, str(path), "bl2"], capture_output=True, text=True)
    lines = r.stdout.splitlines()
    return lines[0] if lines else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--renode", default="renode")
    ap.add_argument("--host-tool", default=str(ROOT / "build" / "bl1_check"))
    ap.add_argument("--bl1", default=str(ROOT / "build" / "bl1_rv32" / "bl1.bin"))
    args = ap.parse_args()

    for p in (args.host_tool, args.bl1):
        if not Path(p).exists():
            sys.exit(f"{p} not found (run `make bl1 bl1-rv32`)")

    manifest = tomllib.loads((VECTORS / "manifest.toml").read_text())
    cases = []
    for v in manifest["vector"]:
        if v["expect"] == "TooShort":
            continue  # shorter than a header: the missing bytes would be stale flash
        path = VECTORS / v["file"]
        status = host_status(args.host_tool, path)
        if status not in STATUS_CODE:
            sys.exit(f"host tool said {status!r} for {v['file']}")
        if status == "Ok" and not v["file"].startswith("valid_bl2"):
            sys.exit(f"unexpected Ok for {v['file']} (expected type is BL2)")
        # Only a valid BL2 image reaches the placement check, and it passes it.
        placement = 1 if status == "Ok" else 0
        cases.append((v["file"], path, STATUS_CODE[status], placement))

    OUT.mkdir(parents=True, exist_ok=True)
    rot = OUT / "rot_peripheral.py"
    rot.write_text(ROT_STUB)
    template = (ROOT / "sim/renode/platform/cerberus.repl.in").read_text()
    repl = OUT / "cerberus.repl"
    repl.write_text(template.replace("@ROT_SCRIPT@", str(rot)))

    lines = [
        "using sysbus",
        'mach create "cerberus-bl1"',
        f"machine LoadPlatformDescription @{repl}",
        "cpu PerformanceInMips 100",
        f"sysbus LoadBinary @{args.bl1} 0x{BL1_BASE:X}",
    ]
    expected = []
    for i, (name, path, code, placement) in enumerate(cases):
        lines.append(f"sysbus LoadBinary @{path} 0x{BL2_BASE:X}")
        lines.append(f"sysbus WriteDoubleWord 0x{REPORT:08X} 0")
        if i > 0:
            lines.append("cpu Reset")
        lines += [f"cpu PC 0x{BL1_BASE:X}", 'emulation RunFor "0.01"']
        lines += [f"sysbus ReadDoubleWord 0x{REPORT + 4 * k:08X}" for k in range(4)]
        for k, (what, want) in enumerate([
                ("report magic", REPORT_MAGIC),
                ("status", code),
                ("placement", placement),
                ("decision", SAFE_HALT)]):
            expected.append({"label": f"{name}: {what}",
                             "address": REPORT + 4 * k, "expected": want})
    lines.append("quit")

    (OUT / "run.resc").write_text("\n".join(lines) + "\n")
    (OUT / "expected.json").write_text(json.dumps(expected, indent=2))
    print(f"[prepare] {len(cases)} vectors, {len(expected)} checks")

    log = OUT / "run.log"
    with open(log, "w") as lf:
        subprocess.run(
            [args.renode, "--disable-xwt", "--console", "--plain", str(OUT / "run.resc")],
            stdout=lf, stderr=subprocess.STDOUT, timeout=600)

    return subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "renode_check.py"),
         str(log), str(OUT / "expected.json")]).returncode


sys.exit(main())