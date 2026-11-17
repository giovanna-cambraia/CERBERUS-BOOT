#!/usr/bin/env python3
"""GHDL <-> Renode bridge for the hardware RoT (ICD-001).

Starts a GHDL simulation of hdl/src/rot_peripheral.vhd behind
sim/bridge/rot_bridge_top.vhd, provisions it, and relays a line protocol
between TCP clients (Renode's RoT peripheral) and the simulation's stdin/stdout:

    P                 -> OK
    R <addr8>         -> OK <data8>
    W <addr8> <data8> -> OK        (all numbers are 8 hex digits)

  python3 sim/bridge/ghdl_renode_socket_bridge.py --port 9999

By default the bridge exits when its last client disconnects (--keep-alive
to stay up). With --port 0 it picks a free port, and the --ready-file
contains the port once the bridge is listening.
"""
import argparse
import hashlib
import os
import re
import socket
import struct
import subprocess
import sys
import threading
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
HDL_SRC = ROOT / "hdl" / "src" / "rot_peripheral.vhd"
TOP_VHD = HERE / "rot_bridge_top.vhd"
REQUEST = re.compile(r"^(P|R [0-9A-Fa-f]{8}|W [0-9A-Fa-f]{8} [0-9A-Fa-f]{8})$")

ROT_COUNTER = 0x024
ROT_LIFECYCLE = 0x028
LIFECYCLE_COMMIT = 0xA5A50001


def otp_words(key_path, flip):
    """ICD-001 section 3.6: word i = digest bytes 4i..4i+3, little-endian."""
    try:
        key = bytes.fromhex(Path(key_path).read_text().strip())
    except (OSError, ValueError) as e:
        sys.exit(f"{key_path}: {e} (run `make sign-keys`)")
    if len(key) != 32:
        sys.exit(f"{key_path}: expected 32 bytes, got {len(key)}")
    words = list(struct.unpack("<8I", hashlib.sha256(key).digest()))
    if flip:
        words[0] ^= 1
    return words


def write_otp_package(workdir, words):
    aggregate = ", ".join(f'x"{w:08X}"' for w in words)
    (workdir / "rot_otp_pkg.vhd").write_text(
        "library ieee;\n"
        "use ieee.std_logic_1164.all;\n"
        "use work.rot_pkg.all;\n\n"
        "package rot_otp_pkg is\n"
        f"    constant OTP_HASH : hash_t := ({aggregate});\n"
        "end package rot_otp_pkg;\n"
    )


def analyse(ghdl, workdir):
    cmd = [ghdl, "-a", "--std=08", f"--workdir={workdir}",
           str(HDL_SRC), str(workdir / "rot_otp_pkg.vhd"), str(TOP_VHD)]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit("GHDL analysis failed:\n" + r.stdout + r.stderr)


class Simulation:
    def __init__(self, ghdl, workdir, stderr_file):
        self.lock = threading.Lock()
        self.proc = subprocess.Popen(
            [ghdl, "--elab-run", "--std=08", f"--workdir={workdir}", "rot_bridge_top"],
            cwd=workdir,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=stderr_file,
            text=True,
            bufsize=1,
        )

    def transact(self, line):
        """Send one request line, return the reply line."""
        with self.lock:
            try:
                self.proc.stdin.write(line + "\n")
                self.proc.stdin.flush()
                for _ in range(20):  # skip any stray runtime output
                    reply = self.proc.stdout.readline()
                    if reply == "":
                        return "ERR simulation ended"
                    reply = reply.strip()
                    if reply.startswith(("OK", "ERR")):
                        return reply
            except OSError:
                return "ERR simulation ended"
        return "ERR no reply"

    def close(self):
        try:
            self.proc.stdin.write("Q\n")
            self.proc.stdin.flush()
            self.proc.stdin.close()
        except OSError:
            pass
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()


def provision(sim, args):
    """Commit the fuses and set the starting counter through the real registers."""
    if not args.no_lock:
        reply = sim.transact(f"W {ROT_LIFECYCLE:08X} {LIFECYCLE_COMMIT:08X}")
        if reply != "OK":
            sys.exit(f"lifecycle commit failed: {reply}")
    if args.counter0:
        reply = sim.transact(f"W {ROT_COUNTER:08X} {args.counter0:08X}")
        if reply != "OK":
            sys.exit(f"counter preset failed: {reply}")
    reply = sim.transact("R 00000000")
    status = int(reply.split()[1], 16) if reply.startswith("OK ") else -1
    want = 0 if args.no_lock else 1
    if status != want:
        sys.exit(f"unexpected RoT status after provisioning: {reply} (wanted 0x{want:08X})")


def serve(sim, args):
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", args.port))
    srv.listen(8)
    port = srv.getsockname()[1]
    if args.ready_file:
        tmp = Path(args.ready_file + ".tmp")
        tmp.write_text(f"{port}\n")
        tmp.replace(args.ready_file)  # atomic: readers never see a partial file
    print(f"LISTENING 127.0.0.1:{port}", flush=True)

    state = {"active": 0, "seen": False}
    state_lock = threading.Lock()

    def handle(conn):
        try:
            with conn, conn.makefile("r", encoding="ascii", newline="\n") as rf, \
                    conn.makefile("w", encoding="ascii", newline="\n") as wf:
                for line in rf:
                    line = line.strip()
                    if not line:
                        continue
                    reply = sim.transact(line) if REQUEST.match(line) else "ERR bad request"
                    wf.write(reply + "\n")
                    wf.flush()
        except OSError:
            pass
        finally:
            with state_lock:
                state["active"] -= 1

    srv.settimeout(0.5)
    while True:
        try:
            conn, _ = srv.accept()
        except socket.timeout:
            with state_lock:
                if state["seen"] and state["active"] == 0 and not args.keep_alive:
                    break
            continue
        with state_lock:
            state["active"] += 1
            state["seen"] = True
        threading.Thread(target=handle, args=(conn,), daemon=True).start()
    srv.close()


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--port", type=int, default=9999)
    ap.add_argument("--key", default=str(ROOT / "keys" / "dev.pub"))
    ap.add_argument("--flip-hash", action="store_true", help="corrupt the OTP hash (wrong-key test)")
    ap.add_argument("--counter0", type=int, default=0, help="initial monotonic counter")
    ap.add_argument("--no-lock", action="store_true", help="leave the fuses uncommitted")
    ap.add_argument("--workdir", default=str(ROOT / "build" / "ghdl_bridge"))
    ap.add_argument("--ghdl", default=os.environ.get("GHDL", "ghdl"))
    ap.add_argument("--ready-file")
    ap.add_argument("--keep-alive", action="store_true")
    args = ap.parse_args()

    workdir = Path(args.workdir).resolve()
    workdir.mkdir(parents=True, exist_ok=True)

    write_otp_package(workdir, otp_words(args.key, args.flip_hash))
    analyse(args.ghdl, workdir)

    with open(workdir / "ghdl.log", "w") as errlog:
        sim = Simulation(args.ghdl, workdir, errlog)
        try:
            provision(sim, args)
            serve(sim, args)
        finally:
            sim.close()


if __name__ == "__main__":
    main()