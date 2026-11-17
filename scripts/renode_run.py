#!/usr/bin/env python3
"""Run one Renode scenario end to end.

  python3 scripts/renode_run.py --rot ghdl --scenario fallback \
      --cli <cerberus-cli> --llvm-bin <dir> --elf <bl2 elf> \
      --renode renode --bridge build/ghdl_renode_socket_bridge --port 9999

With --rot ghdl the VHDL RoT runs behind the bridge, which this script starts
for the duration of the run. With --rot python the scripted RoT model is used.
"""
import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "build" / "renode"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--rot", choices=["python", "ghdl"], default="ghdl")
    ap.add_argument("--scenario", default="normal")
    ap.add_argument("--cli", required=True)
    ap.add_argument("--llvm-bin", required=True)
    ap.add_argument("--elf", required=True)
    ap.add_argument("--renode", default="renode")
    ap.add_argument("--bridge", default="build/ghdl_renode_socket_bridge")
    ap.add_argument("--port", type=int, default=9999)
    args = ap.parse_args()

    OUT.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [sys.executable, "scripts/renode_prepare.py",
         "--scenario", args.scenario, "--rot", args.rot, "--port", str(args.port),
         "--cli", args.cli, "--llvm-bin", args.llvm_bin, "--elf", args.elf],
        cwd=ROOT, check=True, stdout=subprocess.DEVNULL)

    bridge = None
    if args.rot == "ghdl":
        cfg = json.loads((OUT / "bridge.json").read_text())
        ready = OUT / "bridge.ready"
        ready.unlink(missing_ok=True)
        cmd = [str((ROOT / args.bridge).resolve()), "--port", str(args.port),
               "--ready-file", str(ready)]
        if cfg["flip_hash"]:
            cmd.append("--flip-hash")
        if cfg["no_lock"]:
            cmd.append("--no-lock")
        if cfg["counter0"]:
            cmd += ["--counter0", str(cfg["counter0"])]
        bridge_log = open(OUT / "bridge.log", "w")
        bridge = subprocess.Popen(cmd, cwd=ROOT, stdout=bridge_log, stderr=subprocess.STDOUT)
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
                stdout=lf, stderr=subprocess.STDOUT, timeout=300)
    finally:
        if bridge is not None:
            try:
                bridge.wait(timeout=10)  # exits by itself when Renode disconnects
            except subprocess.TimeoutExpired:
                bridge.terminate()
                bridge.wait(timeout=10)

    result = subprocess.run(
        [sys.executable, "scripts/renode_check.py", str(log), str(OUT / "expected.json")],
        cwd=ROOT)
    return result.returncode


sys.exit(main())