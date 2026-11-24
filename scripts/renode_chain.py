#!/usr/bin/env python3
"""Run the whole boot chain in Renode: BL1 (boot ROM) -> BL2 -> application.

  python3 scripts/renode_chain.py --scenario chain-ok --rot python|ghdl \
      --cli <cerberus-cli> --llvm-bin <dir> --elf <BL2 elf> [--renode renode]

BL1 goes into the boot ROM, a (possibly damaged) BL2 image into the BL2
partition, and a good test app into slot A; the hart starts in BL1. Afterwards
BL1's report, BL2's report, the app's marker and both RoT counters are read and
compared with what each scenario should produce.
"""
import argparse
import hashlib
import json
import struct
import subprocess
import sys
import time
from pathlib import Path

import renode_prepare as rp

ROOT = rp.ROOT
OUT = ROOT / "build" / "renode_chain"

BL1_BASE = 0x0001_0000
BL2_BASE = 0x4000_0000
BL1_REPORT = 0x2003_B000
BL1_MAGIC = 0xB101_0002
JUMP, HALT = 0, 1

# Boot.Failure_Code'Pos
(F_NONE, F_HEADER, F_PLACEMENT, F_UNPROVISIONED, F_ROLLBACK,
 F_KEY, F_DIGEST, F_SIGNATURE, F_BURN) = range(9)
# Header.Parse_Status'Pos (only the ones used here)
S_OK, S_CRC, S_WRONG_TYPE = 0, 3, 6

DEFAULTS = dict(locked=1, flip_hash=False, bl2_counter0=0)
SCENARIOS = {
    "chain-ok":               dict(bl2="good"),
    "chain-tampered-payload": dict(bl2="tampered"),   # one payload byte flipped
    "chain-bad-signature":    dict(bl2="badsig"),     # signature bit flipped, CRC fixed
    "chain-bad-header":       dict(bl2="badcrc"),     # header byte flipped, CRC stale
    "chain-attacker-key":     dict(bl2="attacker"),   # re-signed with the attacker's own key
    "chain-wrong-type":       dict(bl2="apptype"),    # BL2 payload signed as an app image
    "chain-wrong-otp":        dict(bl2="good", flip_hash=True),
    "chain-unprovisioned":    dict(bl2="good", locked=0),
    "chain-rollback":         dict(bl2="good", bl2_counter0=2),    # version 1 < counter 2
    "chain-bl2-upgrade":      dict(bl2="secver2", bl2_counter0=1), # version 2 > counter 1
}


def sign_bl2(cli, key, payload, out, image_type, entry, secver=1):
    subprocess.run(
        [str((ROOT / cli).resolve()), "sign",
         "--key", str(key), "--payload", str(payload), "--out", str(out),
         "--type", image_type, "--secver", str(secver),
         "--load", "0x40000100", "--entry", hex(entry)],
        cwd=ROOT, check=True, stdout=subprocess.DEVNULL)


def make_bl2_variant(kind, cli, bl2_bin, bl2_img, entry, out):
    data = bytearray(bl2_img.read_bytes())
    if kind == "good":
        out.write_bytes(data)
    elif kind == "tampered":
        data[0x100 + 0x40] ^= 0x01        # inside the payload, after the 32-byte key
        out.write_bytes(data)
    elif kind == "badsig":
        out.write_bytes(data)
        rp.break_signature(out)
    elif kind == "badcrc":
        data[0x18] ^= 0x01                # a salt byte; the header CRC is not updated
        out.write_bytes(data)
    elif kind == "secver2":
        sign_bl2(cli, ROOT / "keys" / "dev.key", bl2_bin, out, "bl2", entry, secver=2)
    elif kind == "attacker":
        # A well-formed, internally consistent BL2 image, but signed by a key the
        # OTP fuses do not know, with that key embedded at the start of the payload.
        for ext in (".key", ".pub"):
            (OUT / f"attacker{ext}").unlink(missing_ok=True)
        subprocess.run([str((ROOT / cli).resolve()), "keygen", str(OUT / "attacker")],
                       cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
        pub = bytes.fromhex((OUT / "attacker.pub").read_text().strip())
        payload = OUT / "bl2_attacker_payload.bin"
        payload.write_bytes(pub + bl2_bin.read_bytes()[32:])
        sign_bl2(cli, OUT / "attacker.key", payload, out, "bl2", entry)
    elif kind == "apptype":
        sign_bl2(cli, ROOT / "keys" / "dev.key", bl2_bin, out, "app", entry)
    else:
        sys.exit(f"unknown BL2 variant {kind}")


def expectations(name, bl2_entry, bl2_report):
    """List of (label, address, expected value), read after the run."""

    def bl1(status, failure, decision, entry, version, counter):
        return [
            ("BL1 report magic", BL1_REPORT + 0, BL1_MAGIC),
            ("BL1 parse status", BL1_REPORT + 4, status),
            ("BL1 failure code", BL1_REPORT + 8, failure),
            ("BL1 decision", BL1_REPORT + 12, decision),
            ("BL1 entry point", BL1_REPORT + 16, entry),
            ("BL1 BL2 version", BL1_REPORT + 20, version),
            ("BL1 BL2 counter", BL1_REPORT + 24, counter),
        ]

    def bl2(magic, code, slot, entry):
        return [
            ("BL2 report magic", bl2_report + 0, magic),
            ("BL2 report code", bl2_report + 4, code),
            ("BL2 report slot", bl2_report + 8, slot),
            ("BL2 report entry", bl2_report + 12, entry),
        ]

    def counters(app, bl2_counter):
        return [
            ("RoT app counter", rp.ROT_BASE + 0x24, app),
            ("RoT BL2 counter", rp.ROT_BASE + 0x2C, bl2_counter),
        ]

    marker = ("app marker", rp.MARKER, rp.APP_MARKER)

    def booted(version, counter_after):
        # BL1 verifies BL2 and jumps; BL2 boots the app and raises the app counter.
        return (bl1(S_OK, F_NONE, JUMP, bl2_entry, version, counter_after)
                + bl2(rp.REPORT_MAGIC, 0, 0, rp.A_ENTRY)
                + [marker] + counters(1, counter_after))

    def halted(status, failure, version, counter_read, bl2_counter):
        # BL1 stops: BL2 and the app never run, no counter moves.
        return (bl1(status, failure, HALT, 0, version, counter_read)
                + bl2(0, 0, 0, 0)
                + [("app marker", rp.MARKER, 0)] + counters(0, bl2_counter))

    table = {
        "chain-ok":               booted(1, 1),
        "chain-bl2-upgrade":      booted(2, 2),
        "chain-tampered-payload": halted(S_OK, F_DIGEST, 1, 0, 0),
        "chain-bad-signature":    halted(S_OK, F_SIGNATURE, 1, 0, 0),
        "chain-bad-header":       halted(S_CRC, F_HEADER, 0, 0, 0),
        "chain-attacker-key":     halted(S_OK, F_KEY, 1, 0, 0),
        "chain-wrong-type":       halted(S_WRONG_TYPE, F_HEADER, 0, 0, 0),
        "chain-wrong-otp":        halted(S_OK, F_KEY, 1, 0, 0),
        "chain-unprovisioned":    halted(S_OK, F_UNPROVISIONED, 1, 0, 0),
        "chain-rollback":         halted(S_OK, F_ROLLBACK, 1, 2, 2),
    }
    return table[name]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scenario", choices=list(SCENARIOS), default="chain-ok")
    ap.add_argument("--rot", choices=["python", "ghdl"], default="python")
    ap.add_argument("--cli", required=True)
    ap.add_argument("--llvm-bin", required=True)
    ap.add_argument("--elf", required=True, help="the BL2 ELF (for symbols)")
    ap.add_argument("--bl1", default=str(ROOT / "build/bl1_rv32/bl1.bin"))
    ap.add_argument("--bl2-bin", default=str(ROOT / "build/bl2.bin"))
    ap.add_argument("--bl2-img", default=str(ROOT / "build/bl2.img"))
    ap.add_argument("--renode", default="renode")
    ap.add_argument("--bridge", default="build/ghdl_renode_socket_bridge")
    ap.add_argument("--port", type=int, default=9999)
    args = ap.parse_args()

    sc = {**DEFAULTS, **SCENARIOS[args.scenario]}
    bl1_bin, bl2_bin, bl2_img = Path(args.bl1), Path(args.bl2_bin), Path(args.bl2_img)
    for p in (bl1_bin, bl2_bin, bl2_img):
        if not p.exists():
            sys.exit(f"{p} not found (run `make bl2-image bl1-rv32`)")
    OUT.mkdir(parents=True, exist_ok=True)

    bl2_entry = rp.symbol_address(args.llvm_bin, args.elf, "_start")
    bl2_report = rp.symbol_address(args.llvm_bin, args.elf, "BOOT_REPORT")

    # The BL2 image to boot, possibly damaged.
    bl2_variant = OUT / "bl2_variant.img"
    make_bl2_variant(sc["bl2"], args.cli, bl2_bin, bl2_img, bl2_entry, bl2_variant)

    # A good app in slot A, for BL2 to boot when the chain is intact.
    payload = OUT / "app_payload.bin"
    payload.write_bytes(struct.pack("<5I", *rp.APP_WORDS))
    app_a = OUT / "app_a.img"
    rp.make_app("good", args.cli, payload, app_a, 1, rp.A_ENTRY)

    # The RoT: scripted, or the VHDL design behind the bridge.
    rot_py = OUT / "rot_peripheral.py"
    if args.rot == "ghdl":
        rot_py.write_text(rp.ROT_GHDL_TEMPLATE.replace("@PORT@", str(args.port)))
    else:
        digest = hashlib.sha256(rp.read_key()).digest()
        words = list(struct.unpack("<8I", digest))
        if sc["flip_hash"]:
            words[0] ^= 1
        rot_py.write_text(
            rp.ROT_TEMPLATE
            .replace("@WORDS@", ", ".join(f"0x{w:08X}" for w in words))
            .replace("@LOCKED@", str(sc["locked"]))
            .replace("@COUNTER@", "0")
            .replace("@BL2COUNTER@", str(sc["bl2_counter0"])))

    template = (ROOT / "sim/renode/platform/cerberus.repl.in").read_text()
    repl = OUT / "cerberus.repl"
    repl.write_text(template.replace("@ROT_SCRIPT@", str(rot_py)))

    expected = expectations(args.scenario, bl2_entry, bl2_report)
    lines = [
        "using sysbus",
        'mach create "cerberus-chain"',
        f"machine LoadPlatformDescription @{repl}",
        "cpu PerformanceInMips 100",
        f"sysbus LoadBinary @{bl1_bin} 0x{BL1_BASE:X}",
        f"sysbus LoadBinary @{bl2_variant} 0x{BL2_BASE:X}",
        f"sysbus LoadBinary @{app_a} 0x{rp.SLOT_A:X}",
        f"cpu PC 0x{BL1_BASE:X}",
        'emulation RunFor "5"',
    ]
    lines += [f"sysbus ReadDoubleWord 0x{addr:08X}" for _, addr, _ in expected]
    lines.append("quit")
    (OUT / "run.resc").write_text("\n".join(lines) + "\n")
    (OUT / "expected.json").write_text(json.dumps(
        [{"label": n, "address": a, "expected": e} for n, a, e in expected], indent=2))

    # With the VHDL RoT, start the bridge first and wait until it listens.
    bridge = None
    if args.rot == "ghdl":
        ready = OUT / "bridge.ready"
        ready.unlink(missing_ok=True)
        cmd = [str((ROOT / args.bridge).resolve()), "--port", str(args.port),
               "--ready-file", str(ready)]
        if sc["flip_hash"]:
            cmd.append("--flip-hash")
        if not sc["locked"]:
            cmd.append("--no-lock")
        if sc["bl2_counter0"]:
            cmd += ["--bl2-counter0", str(sc["bl2_counter0"])]
        bridge = subprocess.Popen(cmd, cwd=ROOT, stdout=open(OUT / "bridge.log", "w"),
                                  stderr=subprocess.STDOUT)
        deadline = time.time() + 90
        while not ready.exists():
            if bridge.poll() is not None:
                sys.exit(f"bridge exited early, see {OUT / 'bridge.log'}")
            if time.time() > deadline:
                bridge.kill()
                sys.exit("timed out waiting for the bridge")
            time.sleep(0.1)

    log = OUT / f"run-{args.rot}-{args.scenario}.log"
    try:
        with open(log, "w") as lf:
            subprocess.run(
                [args.renode, "--disable-xwt", "--console", "--plain", str(OUT / "run.resc")],
                stdout=lf, stderr=subprocess.STDOUT, timeout=600)
    finally:
        if bridge is not None:
            try:
                bridge.wait(timeout=10)
            except subprocess.TimeoutExpired:
                bridge.terminate()
                bridge.wait(timeout=10)

    return subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "renode_check.py"),
         str(log), str(OUT / "expected.json")]).returncode


sys.exit(main())