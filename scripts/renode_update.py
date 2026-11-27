#!/usr/bin/env python3
"""Over-the-air update scenarios on the whole chain: BL1 -> BL2 -> application.

  python3 scripts/renode_update.py --scenario update-ok --rot python|ghdl \
      --cli <cerberus-cli> --llvm-bin <dir> --elf <BL2 elf> [--renode renode]

Slot A holds application v1, which contains the real OTA agent. The update
stream for v2 sits in a mailbox that stands in for a UART. The device is run,
reset and run again, the way a real device reboots after an update, and after
every boot the BL2 report, the application report, the RoT counters and the
slot-state records are compared with what the scenario must produce.
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
OUT = ROOT / "build" / "renode_update"
APPS = ROOT / "build" / "app_images"

BL1_BASE = 0x0001_0000
BL2_BASE = 0x4000_0000
RX = 0x0004_0000
TX = 0x0005_0000
APP_REPORT = 0x2003_8000
APP_MAGIC = 0xA991_0001
NV = rp.NV_BASE
NV_SECTOR = 0x1000
DATA_CHUNK = 252

CONFIRM_NONE, CONFIRM_DONE, CONFIRM_SELFTEST_FAILED = 0, 1, 2

# The mailbox that stands in for a UART: the update stream in, the replies out.
LINK_REPL = """
ota_rx: Memory.MappedMemory @ sysbus 0x00040000
    size: 0x10000

ota_tx: Memory.MappedMemory @ sysbus 0x00050000
    size: 0x10000
"""

SCENARIOS = {
    "update-ok":              dict(app="v2_b",    mutate=None),
    "update-corrupt":         dict(app="v2_b",    mutate="corrupt"),   # one stream byte damaged
    "update-bad-signature":   dict(app="v2_b",    mutate="badsig"),    # staged, then refused by BL2
    "update-failed-selftest": dict(app="v2bad_b", mutate=None),        # trial boots, never confirmed
}


def bl2_reads(report, code, slot, entry):
    return [
        ("BL2 report magic", report, rp.REPORT_MAGIC),
        ("BL2 report code", report + 4, code),
        ("BL2 report slot", report + 8, slot),
        ("BL2 report entry", report + 12, entry),
    ]


def app_reads(version, slot, trial, confirm, staged, consumed):
    return [
        ("app report magic", APP_REPORT, APP_MAGIC),
        ("app version", APP_REPORT + 4, version),
        ("app slot", APP_REPORT + 8, slot),
        ("app trial boot", APP_REPORT + 12, trial),
        ("app confirm result", APP_REPORT + 16, confirm),
        ("app update staged", APP_REPORT + 20, staged),
        ("app bytes consumed", APP_REPORT + 24, consumed),
    ]


def counters(app, bl2):
    return [
        ("RoT app counter", rp.ROT_BASE + 0x24, app),
        ("RoT BL2 counter", rp.ROT_BASE + 0x2C, bl2),
    ]


def nv_reads(sector, seq, state):
    base = NV + sector * NV_SECTOR
    return [
        (f"NV sector {sector}: seq", base + 0x8, seq),
        (f"NV sector {sector}: state", base + 0xC, state),
    ]


def tx_reads(length=None):
    # ACK frame: C3 5A 81 00 ... read as a little-endian word.
    reads = [("reply mailbox: first reply is an ACK", TX + 4, 0x00815AC3)]
    if length is not None:
        reads.insert(0, ("reply mailbox: bytes written", TX, length))
    return reads


# NV state word = active | slot_a << 8 | slot_b << 16 | trials_left << 24
# Slot status: Empty 0, Pending 1, Testing 2, Confirmed 3, Invalid 4, Broken 5.
def phases_for(name, stream_len, nframes, bl2_report):
    A, B = rp.A_ENTRY, rp.B_ENTRY

    def bl2(code, slot, entry):
        return bl2_reads(bl2_report, code, slot, entry)

    # v1 boots normally from slot A and stages the update: B is Pending.
    stage = (bl2(0, 0, A)
             + app_reads(1, 0, 0, CONFIRM_NONE, 1, stream_len)
             + counters(1, 1)
             + nv_reads(1, 2, 0x00010300)
             + tx_reads(16 * nframes))

    if name == "update-ok":
        return [
            stage,
            # BL2 trial-boots v2 from slot B, which confirms itself.
            bl2(1, 1, B) + app_reads(2, 1, 1, CONFIRM_DONE, 0, 0)
            + counters(1, 1) + nv_reads(1, 4, 0x00030301),
            # The next boot is normal, and the application counter reaches 2.
            bl2(0, 1, B) + app_reads(2, 1, 0, CONFIRM_NONE, 0, 0)
            + counters(2, 1) + nv_reads(1, 4, 0x00030301),
        ]

    if name == "update-corrupt":
        # The damaged frame is dropped, nothing is staged, and slot B stays Empty.
        return [
            bl2(0, 0, A) + app_reads(1, 0, 0, CONFIRM_NONE, 0, stream_len)
            + counters(1, 1) + nv_reads(0, 1, 0x00000300) + tx_reads(),
            bl2(0, 0, A) + app_reads(1, 0, 0, CONFIRM_NONE, 0, 0)
            + counters(1, 1) + nv_reads(0, 1, 0x00000300),
        ]

    if name == "update-bad-signature":
        # The untrusted agent stages the forged image; BL2 refuses to boot it.
        return [
            stage,
            bl2(0, 0, A) + app_reads(1, 0, 0, CONFIRM_NONE, 0, 0)
            + counters(1, 1) + nv_reads(0, 3, 0x00040300),
        ]

    if name == "update-failed-selftest":
        def trial(sector, seq, trials_left):
            return (bl2(1, 1, B)
                    + app_reads(2, 1, 1, CONFIRM_SELFTEST_FAILED, 0, 0)
                    + counters(1, 1)
                    + nv_reads(sector, seq, 0x00020300 | (trials_left << 24)))
        return [
            stage,
            trial(0, 3, 2),   # first trial boot
            trial(1, 4, 1),   # second
            trial(0, 5, 0),   # third
            # The trials are used up: B is marked Broken and A boots normally.
            bl2(0, 0, A) + app_reads(1, 0, 0, CONFIRM_NONE, 0, 0)
            + counters(1, 1) + nv_reads(1, 6, 0x00050300),
        ]

    sys.exit(f"no expectations for {name}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--scenario", choices=list(SCENARIOS), default="update-ok")
    ap.add_argument("--rot", choices=["python", "ghdl"], default="python")
    ap.add_argument("--cli", required=True)
    ap.add_argument("--llvm-bin", required=True)
    ap.add_argument("--elf", required=True, help="the BL2 ELF (for symbols)")
    ap.add_argument("--bl1", default=str(ROOT / "build/bl1_rv32/bl1.bin"))
    ap.add_argument("--bl2-img", default=str(ROOT / "build/bl2.img"))
    ap.add_argument("--renode", default="renode")
    ap.add_argument("--bridge", default="build/ghdl_renode_socket_bridge")
    ap.add_argument("--port", type=int, default=9999)
    args = ap.parse_args()

    sc = SCENARIOS[args.scenario]
    bl1_bin, bl2_img = Path(args.bl1), Path(args.bl2_img)
    v1_bin, v2_bin = APPS / "v1_a.bin", APPS / f"{sc['app']}.bin"
    for p in (bl1_bin, bl2_img, v1_bin, v2_bin):
        if not p.exists():
            sys.exit(f"{p} not found (run `make bl2-image bl1-rv32 app-images`)")
    OUT.mkdir(parents=True, exist_ok=True)

    bl2_report = rp.symbol_address(args.llvm_bin, args.elf, "BOOT_REPORT")
    cli = str((ROOT / args.cli).resolve())

    # Signed application images, each linked for the slot it runs from.
    app_v1, app_v2 = OUT / "app_v1.img", OUT / "app_v2.img"
    rp.sign(args.cli, v1_bin, app_v1, 1, rp.SLOT_A + 0x100)
    rp.sign(args.cli, v2_bin, app_v2, 2, rp.SLOT_B + 0x100)
    if sc["mutate"] == "badsig":
        rp.break_signature(app_v2)       # signature bit flipped, header CRC fixed

    # The update stream for v2, as the Rust tool frames it.
    frames = OUT / "update.frames"
    subprocess.run([cli, "ota-frames", str(app_v2), str(frames)],
                   cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    data = bytearray(frames.read_bytes())
    if sc["mutate"] == "corrupt":
        data[30] ^= 0x01                 # inside the first DATA frame
    if len(data) > 0x10000 - 4:
        sys.exit("the update stream does not fit the mailbox")
    nframes = 2 + (app_v2.stat().st_size + DATA_CHUNK - 1) // DATA_CHUNK
    rx = OUT / "rx.bin"
    rx.write_bytes(struct.pack("<I", len(data)) + bytes(data))

    # The RoT: scripted, or the VHDL design behind the bridge.
    rot_py = OUT / "rot_peripheral.py"
    if args.rot == "ghdl":
        rot_py.write_text(rp.ROT_GHDL_TEMPLATE.replace("@PORT@", str(args.port)))
    else:
        words = struct.unpack("<8I", hashlib.sha256(rp.read_key()).digest())
        rot_py.write_text(
            rp.ROT_TEMPLATE
            .replace("@WORDS@", ", ".join(f"0x{w:08X}" for w in words))
            .replace("@LOCKED@", "1")
            .replace("@COUNTER@", "0")
            .replace("@BL2COUNTER@", "0"))

    template = (ROOT / "sim/renode/platform/cerberus.repl.in").read_text()
    repl = OUT / "cerberus.repl"
    repl.write_text(template.replace("@ROT_SCRIPT@", str(rot_py)) + LINK_REPL)

    phases = phases_for(args.scenario, len(data), nframes, bl2_report)
    lines = [
        "using sysbus",
        'mach create "cerberus-update"',
        f"machine LoadPlatformDescription @{repl}",
        "cpu PerformanceInMips 100",
        f"sysbus LoadBinary @{bl1_bin} 0x{BL1_BASE:X}",
        f"sysbus LoadBinary @{bl2_img} 0x{BL2_BASE:X}",
        f"sysbus LoadBinary @{app_v1} 0x{rp.SLOT_A:X}",
        f"sysbus LoadBinary @{rx} 0x{RX:X}",
    ]
    for i, phase in enumerate(phases):
        if i > 0:
            # A reboot: the CPU restarts, flash and RoT keep their state, and
            # the update stream has been consumed.
            lines += ["cpu Reset", f"sysbus WriteDoubleWord 0x{RX:08X} 0"]
        lines += [
            f"sysbus WriteDoubleWord 0x{bl2_report:08X} 0",
            f"sysbus WriteDoubleWord 0x{APP_REPORT:08X} 0",
            f"cpu PC 0x{BL1_BASE:X}",
            'emulation RunFor "8"',
        ]
        lines += [f"sysbus ReadDoubleWord 0x{addr:08X}" for _, addr, _ in phase]
    lines.append("quit")
    (OUT / "run.resc").write_text("\n".join(lines) + "\n")

    expected = []
    for i, phase in enumerate(phases):
        expected += [{"label": f"boot {i + 1}: {n}", "address": a, "expected": e}
                     for n, a, e in phase]
    (OUT / "expected.json").write_text(json.dumps(expected, indent=2))

    bridge = None
    if args.rot == "ghdl":
        ready = OUT / "bridge.ready"
        ready.unlink(missing_ok=True)
        bridge = subprocess.Popen(
            [str((ROOT / args.bridge).resolve()), "--port", str(args.port),
             "--ready-file", str(ready)],
            cwd=ROOT, stdout=open(OUT / "bridge.log", "w"), stderr=subprocess.STDOUT)
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
                stdout=lf, stderr=subprocess.STDOUT, timeout=1800)
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