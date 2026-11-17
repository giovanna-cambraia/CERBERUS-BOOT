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