# ADR-003: Transport between Renode and the GHDL simulation

- **Status:** Accepted
- **Context:** The hardware RoT is a VHDL design (`hdl/src/rot_peripheral.vhd`)
  simulated by GHDL. Renode emulates the CPU and needs to read and write the
  RoT's registers. ASM-001 proposed Unix domain sockets between them.
- **Decision:** Renode's RoT peripheral (a Python peripheral) talks over
  loopback TCP to a bridge process. The bridge runs GHDL as a child process and
  talks to it through stdin/stdout, using VHDL `textio`. The protocol is
  one ASCII line per access.
- **Consequences:**
  - Positive: no C code, no VHPI/VPI, works with any GHDL backend, and needs
    no native build step.
  - Positive: the simulation's state lives in one process, so the fuses and
    counter keep their non-volatile behaviour regardless of what happens to
    the emulated CPU.
  - Negative: TCP loopback instead of Unix domain sockets (deviation from
    ASM-001), because IronPython inside Renode has no reliable support for
    them.
  - Negative: each register access costs a few process-to-process round
    trips, which is fine for the handful of accesses a boot makes, and not
    suitable for a peripheral hit in a tight loop.
  - Negative: the VHDL clock does not advance between accesses, so time-based
    hardware behaviour would need a different coupling (the RoT has none).