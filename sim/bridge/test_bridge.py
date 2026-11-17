#!/usr/bin/env python3
"""Tests the GHDL bridge without Renode: start it, talk to it the way Renode's
RoT peripheral does, and check the ICD-001 behaviour of the VHDL design."""
import hashlib
import socket
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
BRIDGE = HERE / "ghdl_renode_socket_bridge.py"
KEY = ROOT / "keys" / "dev.pub"

failures = []
checks = 0


def check(name, got, want):
    global checks
    checks += 1
    if got != want:
        failures.append(name)
        print(f"  [FAIL] {name}: got 0x{got:08X}, want 0x{want:08X}")
    else:
        print(f"  [ok]   {name}")


def expected_words(flip=False):
    key = bytes.fromhex(KEY.read_text().strip())
    words = list(struct.unpack("<8I", hashlib.sha256(key).digest()))
    if flip:
        words[0] ^= 1
    return words


class Session:
    def __init__(self, *extra):
        self.tmp = tempfile.TemporaryDirectory()
        tmp = Path(self.tmp.name)
        ready = tmp / "ready"
        self.log_path = tmp / "bridge.log"
        self.log = open(self.log_path, "w")
        cmd = [sys.executable, str(BRIDGE), "--port", "0",
               "--ready-file", str(ready), "--workdir", str(tmp / "work"), *extra]
        self.proc = subprocess.Popen(cmd, cwd=ROOT, stdout=self.log, stderr=subprocess.STDOUT)
        deadline = time.time() + 90
        while not ready.exists():
            if self.proc.poll() is not None:
                self.log.flush()
                sys.exit("bridge exited early:\n" + self.log_path.read_text())
            if time.time() > deadline:
                self.proc.kill()
                sys.exit("bridge did not become ready")
            time.sleep(0.1)
        port = int(ready.read_text().split()[0])
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=30)
        self.f = self.sock.makefile("rw", encoding="ascii", newline="\n")

    def transact(self, line):
        self.f.write(line + "\n")
        self.f.flush()
        reply = self.f.readline().strip()
        if not reply.startswith("OK"):
            sys.exit(f"bridge replied {reply!r} to {line!r}")
        return reply

    def read(self, off):
        return int(self.transact(f"R {off:08X}").split()[1], 16)

    def write(self, off, value):
        self.transact(f"W {off:08X} {value:08X}")

    def close(self):
        self.f.close()
        self.sock.close()
        try:
            self.proc.wait(timeout=30)  # the bridge exits after its last client leaves
        except subprocess.TimeoutExpired:
            self.proc.kill()
        self.log.close()
        self.tmp.cleanup()


def main():
    if not KEY.exists():
        sys.exit(f"{KEY} not found (run `make sign-keys`)")
    words = expected_words()

    print("provisioned device:")
    s = Session()
    check("status after commit (locked)", s.read(0x000), 0x1)
    for i, w in enumerate(words):
        check(f"hash word {i}", s.read(0x004 + 4 * i), w)
    check("counter starts at 0", s.read(0x024), 0)
    s.write(0x024, 5)
    check("counter burned to 5", s.read(0x024), 5)
    s.write(0x024, 3)
    check("decrement rejected", s.read(0x024), 5)
    check("error flag set", s.read(0x000), 0x3)
    s.write(0x004, 0xDEADBEEF)
    check("hash register read-only", s.read(0x004), words[0])
    check("unmapped read is 0", s.read(0x100), 0)
    s.close()

    print("uncommitted fuses (--no-lock):")
    s = Session("--no-lock")
    check("status not locked", s.read(0x000), 0x0)
    s.write(0x028, 0xA5A50001)
    check("commit through the register", s.read(0x000), 0x1)
    s.close()

    print("wrong OTP hash (--flip-hash):")
    s = Session("--flip-hash")
    wrong = expected_words(flip=True)
    for i in (0, 1):
        check(f"hash word {i}", s.read(0x004 + 4 * i), wrong[i])
    s.close()

    print("initial counter (--counter0 2):")
    s = Session("--counter0", "2")
    check("counter preset", s.read(0x024), 2)
    check("status clean", s.read(0x000), 0x1)
    s.close()

    if failures:
        print(f"[FAIL] {len(failures)} of {checks} checks failed")
        return 1
    print(f"[ok] {checks} checks passed")
    return 0


sys.exit(main())