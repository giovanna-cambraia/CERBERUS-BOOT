# Renode end-to-end scenarios

Run with `make renode-all` (one scenario: `make renode-run SCENARIO=<name>`).
Each scenario boots BL2 on the emulated RV32IMAC platform and compares the
boot report, the RoT counter and the NV slot record with fixed expectations.
BL1 is not in the chain yet, so these cover the BL2 stage only.

| Scenario | Requirement | STRIDE | Expected outcome |
|---|---|---|---|
| normal | verified boot of an app | | boots A, counter burned to the app version |
| update | REQ-003 | | pending image boots on trial, NV records Testing |
| tampered | image integrity | Tampering | halts, slot marked Invalid, app does not run |
| rollback | REQ-002 | Elevation of privilege | halts, old signed image refused |
| wrong-otp | trust anchor | Spoofing | halts without changing NV |
| unprovisioned | lifecycle | | halts, NV unchanged |
| bad-update | forged OTA image | Spoofing | image rejected, previous slot keeps booting |
| fallback | REQ-003 | Denial of service | three trial boots, then recovery to the old slot |

Not covered yet: power loss during an update (covered at the unit level by
the NV and boot-selection sweeps in `firmware/bl2_rust/tests`), BL1 verifying
BL2 (REQ-001), and the GHDL-backed RoT.

## Full chain (BL1 -> BL2 -> app)

Run with `make chain-all` (`ROT=ghdl` for the VHDL root of trust). BL1 runs from
the boot ROM and verifies BL2 before jumping to it.

| Scenario | Requirement | STRIDE | Expected outcome |
|---|---|---|---|
| chain-ok | REQ-001 | | BL1 verifies BL2 and jumps; BL2 boots the app |
| chain-tampered-payload | REQ-001 | Tampering | BL1 stops at the digest check, BL2 never runs |
| chain-bad-signature | REQ-001 | Spoofing | BL1 stops at the signature check |
| chain-bad-header | REQ-001 | Tampering | BL1 rejects the header (CRC) |
| chain-attacker-key | REQ-001 | Spoofing | a BL2 re-signed with another key stops at the key-hash check |
| chain-wrong-type | REQ-001 | | an image of the wrong type is rejected |
| chain-wrong-otp | REQ-001 | Spoofing | OTP hash differs from BL2's key: halt |
| chain-unprovisioned | lifecycle | | fuses not committed: halt |
| chain-rollback | REQ-002 | Elevation of privilege | BL2 version 1 with BL2 counter 2: BL1 refuses, BL2 never runs |
| chain-bl2-upgrade | REQ-002 | | BL2 version 2 with counter 1: boots, and BL1 raises the BL2 counter to |

## Updates over the emulated link (BL1 -> BL2 -> application)

Run with `make update-all` (`ROT=ghdl` for the VHDL root of trust). Slot A runs
application v1, which contains the OTA agent. The update stream for v2 is in a
mailbox that stands in for a UART. The device is reset between phases.

| Scenario | Requirement | STRIDE | Expected outcome |
|---|---|---|---|
| update-ok | REQ-003, REQ-005 | | v1 stages v2; BL2 trial-boots it from slot B; v2 confirms itself; the next boot is normal and the application counter reaches 2 |
| update-corrupt | REQ-005 | Tampering | a damaged stream is never staged: slot B stays empty and v1 keeps booting |
| update-bad-signature | REQ-001, REQ-003 | Spoofing | the untrusted agent stages a forged image, and BL2 marks it invalid and boots v1 |
| update-failed-selftest | REQ-003 | Denial of service | v2's self-test fails: three trial boots without confirmation, then B is marked broken and v1 boots |

Platform additions for these scenarios, not in ASM-003: a receive mailbox at
`0x0004_0000`, a reply mailbox at `0x0005_0000` (64 KB each, the first word of
each is a length), and the application's report at `0x2003_8000`.