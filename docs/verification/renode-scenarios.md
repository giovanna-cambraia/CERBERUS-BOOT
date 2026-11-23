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

Not covered: anti-rollback for BL2 itself. BL1 does not compare BL2's security
version with the hardware counter, so an older, validly signed BL2 would boot.