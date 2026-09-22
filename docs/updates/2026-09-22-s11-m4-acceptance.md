# 2026-09-22 — S11 M4 release acceptance

## Decision

- **M4 approved; S11 accepted; the S00–S11 delivery is complete (12 of 12
  stages accepted).**
- Reviewer / date: `simplysoup` / 2026-09-22.
- Scope: acceptance of the S11 release-validation stage and its evidence.
  Publishing or distributing artifacts remains a separate, not-yet-authorized
  action.

## Accepted gate evidence

| Gate | Record |
| --- | --- |
| S11-AC1 | [Clean-checkout package identity](2026-09-22-s11-clean-checkout-ac1.md) |
| S11-AC2 | [Release-candidate recovery matrix](2026-09-22-s11-ac2-recovery-matrix.md) |
| S11-AC3 | [pmon-backed benchmark update](2026-09-22-s11-benchmark-pmon-reruns.md) + [result-review disposition](2026-09-22-s11-result-review-disposition.md) |
| S11-AC4 | [Packaged-layout operator verification](2026-09-22-s11-ac4-ac5-docs-licensing.md) |
| S11-AC5 | [Licensing/provider-terms review](2026-09-22-s11-ac4-ac5-docs-licensing.md) |

Release candidate: source revision `7efa663f…`, package digest
`37c3a3290263b5b29f261e55b4f87b31e625facf19349b8f5f618e011e8e09dc`,
`worldwall` SHA-256 `a75cbb61…70596212`, `worldwallctl` SHA-256
`ea358c34…1313e8e6`, `bundled_artwork=[]`.

## Accepted non-blocking limitations (carried into any future release notes)

1. Single validated host (Hyprland 0.56.2, Vulkan 1.4.357, NVIDIA RTX 3070 Ti,
   two 1920×1080 outputs, scale 1); no universal hardware-compatibility claim.
2. CR-0004: physical incompatible-capability output evidence is future work
   (`not-run`).
3. CR-0002 assigned live multi-variant sequence remains a follow-up gate.
4. B9 zero-output workload `not-run` on this compositor (RB-0003 limitation);
   B10 suspend pmon sampling `not-run` (S08 suspend/resume traces are the
   suspend evidence).
5. Live offline-weather fault and release-binary device/OOM injection were not
   observed; their deterministic coverage is in recorded tests.
6. Application-level allocation counters only; no driver allocator/VRAM
   statistics; no energy-savings claim.
7. Whole-board power figures are host-level observational data, not
   per-process measurements.

## Follow-up proposals (open, not part of M4)

- Authorization and mechanism for publishing artifacts (tarball/package), with
  an installation vector decision.
- A valid zero-output host procedure if B9 is ever to be measured.
- Optional user-service path only if a tested session-environment design is
  approved.

## Reviewer note

Recorded under the standing instruction that unqualified approvals use the
`simplysoup` identity. Stage-file checkboxes intentionally remain untouched;
stage state is recorded in the roadmap and this ledger per project convention.
