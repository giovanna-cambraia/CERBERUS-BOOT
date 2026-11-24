# ADR-005: What "verified" means for BL1

- **Status:** Accepted
- **Context:** BL1 is written in Ada/SPARK and analysed with GNATprove. A
  statement like "formally verified" is only meaningful if it says exactly what
  was proved and what was assumed.
- **Decision:** The claim is limited to the list below, and the shipped ROM is
  built on the condition that `make prove` is fully green.

## Proved

For the SPARK units `BL1.Types`, `BL1.CRC`, `BL1.Header`, `BL1.Policy`,
`BL1.SHA256`, `BL1.Boot` and the wrapper in `BL1.Ed25519`:

- absence of run-time errors: array index, range, overflow and division checks,
  and initialization of every variable (flow analysis);
- `Header.Parse`: an accepted header has the expected image type and a payload
  size within bounds;
- `Header.Placement_Ok`: a placed payload fits the partition, which is what
  keeps every flash read inside it (the precondition of `Platform.Read_Payload`);
- `Policy.Decide`: it returns "jump" if and only if all seven checks passed;
- `Boot.Check_BL2`: a "jump" decision exists only when no failure is recorded;
- `Boot.Finish_BL2`: it can only make the outcome more restrictive.

## Tested, not proved

- That SHA-256 and the CRC compute the right values (known answers, hashlib, a
  sweep of every padding boundary, and every test vector).
- That the parser's verdicts match the other implementations (`make test-bl1`,
  `make renode-bl1`).
- That Monocypher agrees with RFC 8032 and with the Rust implementation on the
  vectors (`make test-ed25519`).

## Assumed

- Monocypher computes Ed25519 verification correctly. It is C, outside the proof,
  and called through a contract that only states that it reads its arguments.
- The bodies of `BL1.Platform` (memory-mapped reads and writes, about 80
  lines), `bl1_main`, the start-up and jump assembly, and the three-function C
  runtime behave as their specifications say.
- The hardware behaves as the contracts say: flash and RoT reads return defined
  values, and the RoT counters are forward-only.
- The compiler, the runtime derived at build time, the linker script and the
  emulator are correct.

## Consequences

- The ROM is compiled with all run-time checks suppressed (`-gnatp`). That is
  sound only for code GNATprove has proved free of run-time errors. A unit that
  is red or unproved in `make prove` invalidates the claim for that unit.
- Any change to a SPARK unit, to GNATprove's version, to the `Platform` body or
  to the runtime derivation requires re-running `make prove` and
  `make verify`.
- What the claim does **not** say: that BL1 is free of every defect, that
  Ed25519 is implemented correctly, or that the hardware cannot be attacked
  physically.