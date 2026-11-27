# CERBERUS-BOOT

A host-emulated secure boot chain with a hardware root of trust: boot ROM,
second-stage bootloader, signed A/B updates with rollback protection, and the
tests and proofs that check each link. It runs natively on Linux (WSL2) with no
containers.

> **Status: a simulation and a learning/portfolio project.** Nothing has run on
> real hardware. Keys in this repository are development keys. It makes no
> certification claim. `docs/compliance/traceability.md` says what is covered,
> what is partial and what is still open.

## Architecture

```text
              Renode (RV32IMAC hart)                        GHDL
  +----------------------------------------------+   TCP  +------------------+
  | Boot ROM   BL1  Ada/SPARK       0x0001_0000  |  and   | rot_peripheral   |
  |   verifies BL2: OTP, SHA-256, Ed25519        | <----> | (VHDL, ICD-001)  |
  |   |                                          | pipes  |  OTP key hash    |
  |   v                                          |        |  2 counters      |
  | BL2 partition  BL2 Rust no_std  0x4000_0000  |        |  lifecycle lock  |
  |   verifies the app, runs the A/B state       |        +------------------+
  |   machine                                    |
  |   |                                          |
  |   v                                          |
  | Slot A / Slot B  app images                  |
  |        0x4002_0000 / 0x4010_0000             |
  | NV slot state (two 4 KB sectors) 0x401E_0000 |
  +----------------------------------------------+
```

Boot sequence:

1. The hart starts in **BL1**, in the boot ROM. BL1 checks the BL2 image header,
   its placement, that the RoT fuses are committed, BL2's anti-rollback
   counter, that the SHA-256 of BL2's embedded key equals the hash in the OTP
   fuses, the SHA-256 of the payload, and the Ed25519 signature. Only if all
   seven pass does it raise the BL2 counter and jump.
2. **BL2** verifies the application slots the same way (against its own counter),
   runs the A/B trial-boot state machine, raises the application counter for
   confirmed images, and jumps to the application.
3. Any failed check ends in a fail-stop halt with a numeric reason.

Details: `docs/architecture.md`.

## Components

| Component | Language | State |
|---|---|---|
| Root of trust: OTP key hash, two monotonic counters, lifecycle lock | VHDL (GHDL) | implemented, tested |
| BL1 boot ROM | Ada/SPARK, with C (Monocypher, start-up) | implemented, proved, tested |
| BL2 | Rust `no_std` | implemented, tested |
| `cerberus-cli`: keys, signing, verification, test vectors | Rust (host) | implemented |
| OTA agent: frame parser, slot-state writer, update logic | C11 | implemented, tested on the host and on the emulated target |
| Test application that confirms an update | C11 | implemented (runs a self-test, then calls `confirm`) |
| Fuzzing (ASan/UBSan) of the OTA parser and agent | C | implemented |
| MISRA C:2012 analysis of the agent | C | tool wired, findings not triaged |
| Real application that confirms an update | C11 | **not started** |
| Kani harnesses, fuzzing of the OTA parser | | **not started** |

## Setup

Tested layout: Windows with WSL2 and Ubuntu 24.04. Work inside the Linux
filesystem (`~/cerberus-boot`), not under `/mnt/c`.

```bash
sudo apt install -y build-essential git ghdl clang tree python3 gnat gprbuild \
                    unzip wget curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
     --profile minimal -c clippy -c rustfmt
source "$HOME/.cargo/env"
rustup target add riscv32imac-unknown-none-elf
rustup component add llvm-tools
```

Three more tools live under `~/tools` and need to be on `PATH`: Renode (the
portable Linux build), GNATprove, and the `riscv64-elf` GNAT cross compiler
(both from Alire: `alr get gnatprove`, `alr get gnat_riscv64_elf`). The
Ada/SPARK parts also need the Monocypher files vendored as described in
`firmware/bl1_spark/third_party/monocypher/VENDORED.md`. Then:

```bash
make check-tools     # reports what is installed and what is missing
make sign-keys       # development signing keys in keys/ (never committed)
make vectors         # deterministic test vectors in test/vectors/
```

## Checking everything

```bash
make verify-quick    # every host-side check, no emulator (includes the proofs)
make verify          # everything, including all Renode scenarios
```

| Command | What it checks |
|---|---|
| `make spec-check` | header and NV-record specs are consistent, and match the Rust constants |
| `make test-hdl` | the VHDL root of trust, in its own testbench |
| `make bridge-test` | the same VHDL driven through the Renode bridge protocol |
| `make test-cli` | header encoding, signing and verification in the host tool |
| `make test-bl2` | BL2: parser, verification, state machine, power-loss sweeps, boot flow |
| `make prove` | GNATprove on BL1: no run-time errors, plus the decision properties |
| `make test-bl1`, `make test-sha256`, `make test-ed25519` | BL1's parser, SHA-256 and Ed25519 against the shared vectors, hashlib and RFC 8032 |
| `make renode-bl1` | the same BL1 source on RV32 gives the host build's verdicts |
| `make renode-all` | BL2 boots, updates, fails and recovers, on the emulated platform |
| `make cosim-all` | the same, with the VHDL root of trust |
| `make chain-all` | the whole chain from the boot ROM, with damage at every link |
| `make docs-check` | the documents only mention commands, files and scenarios that exist |
| `make test-app` | OTA agent: slot-state writer (golden record, power-loss sweep), frame parser, update logic, and the Rust frame encoder against the C one |
| `make fuzz-app` | the OTA parser and agent under ASan and UBSan, on mutated frames |
| `make lint-app` | cppcheck with the MISRA C:2012 addon on the agent (informational) |

Full test plan: `docs/verification/VTP-001.md`.

## Documents

| Document | Purpose |
|---|---|
| `docs/architecture.md` | memory map, boot sequence, check order, data structures |
| `docs/compliance/traceability.md` | requirements to design to evidence, with gaps |
| `docs/threat-model/stride.md` | threats, mitigations built, evidence, residual risk |
| `docs/verification/VTP-001.md` | the verification plan and how to run it |
| `docs/verification/renode-scenarios.md` | each emulator scenario and what it shows |
| `docs/interfaces/ICD-001_rot_peripheral.md` | the root-of-trust register interface |
| `firmware/common/spec/image_header.md`, `firmware/common/spec/nv_state.md` | the two shared data formats |
| `docs/adr/` | architecture decisions, including the proof boundary (ADR-005) |

## Layout

```text
hdl/                  VHDL root of trust and its testbench
firmware/bl1_spark/   boot ROM: SPARK sources, C start-up, vendored Monocypher
firmware/bl2_rust/    BL2 library, tests, and the bare-metal entry crate
firmware/common/      shared specs: image header, NV record
tools/cerberus-cli/   host tool: keygen, sign, verify, vectors
sim/                  GHDL<->Renode bridge, Renode platform description
scripts/              build, run and check scripts
test/vectors/         deterministic signed and damaged images, with a manifest
docs/                 documents (see above)
```

## Known limitations

- The update link is a memory-mapped mailbox standing in for a UART, with a
  preloaded stream and no retransmission, and the application is a small test
  program. The MISRA analysis of the C code is wired but its findings are not
  triaged (REQ-005 is partial).
- No Kani harness: `make kani` is a placeholder (REQ-004 is partial).
- "Verified" has a precise, limited meaning for BL1: see ADR-005. Monocypher, the
  hardware-access code and the assembly are outside the proof.
- Flash is modelled as plain memory, and the boot ROM is not write-protected.
  Power loss is injected in unit tests, not in the emulator.
- Single flat vendor key, no revocation, no firmware encryption, no
  fault-injection defences, no hardware yet.

## Licence

None chosen yet. Third-party code keeps its own licence (Monocypher: see
`firmware/bl1_spark/third_party/monocypher/VENDORED.md`).