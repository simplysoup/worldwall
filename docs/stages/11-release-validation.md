# S11 — Recovery, performance, and release acceptance (M4)

## Purpose

Turn an integrated prototype into an evidence-backed release candidate without an unbounded “polish” phase. Requirements: WW-01–WW-12. Dependency: S10 and all earlier accepted gates. State/owner: [roadmap](../roadmap.md).

## Entry conditions

All preceding stages accepted with current evidence; no stale dependencies. Maintainer approves a named hardware/workload matrix and numeric CPU/memory/pacing/GPU/power/network budgets **before** benchmarking. Licensing/redistribution and unresolved critical defects must be reviewed.

## Deliverables

Release validation report, fault/recovery matrix, approved performance report, clean-install/run/control/troubleshooting documentation, asset provenance/license inventory, and maintainer release decision. Publishing a release is a separate explicitly authorized action.

## Execution

1. Reproduce setup/build/test from a clean checkout using the locked toolchain/dependencies. Verify docs and controls on the packaged layout, not only the development tree.
2. Re-run critical lifecycle/selection/color tests on the approved matrix. Include mixed outputs, no outputs, repeated startup/shutdown, suspend/resume, offline startup, slow/corrupt weather, invalid assets/config, and hostile/malformed local IPC.
3. Inject lost/outdated/timeout/device/OOM cases where feasible; identify precisely which results are simulated versus observed on hardware. Verify bounded recovery/clean exit and useful diagnostics.
4. Run [the measurement procedure](../validation.md): idle/static/animated 15 and 30 FPS, one/multiple outputs, 60-second warmup and three 5-minute samples. Preserve raw data and compare to approved budgets and S04 baseline.
5. Review full-upload/selection counters and network cadence; demonstrate that weather never changes families and, only with valid CR-0006 opt-in, fresh weather changes at most a same-family precipitation variant; no normal frame regenerates image data.
6. Document foreground launch, Hyprland `exec-once`, stop/restart, config and scene authoring, CLI, cache/privacy, supported versions/hardware, and recovery. A user service is optional and only included with a tested session-environment path and no duplicate-launch instructions.
7. Check software license, every bundled asset's provenance/attribution, provider terms, secret-free examples, and artifact reproducibility. No downloads or publication are implicit.
8. Submit all gate evidence, limitations, regressions, and unresolved issues for maintainer review. Update status/roadmap only after explicit acceptance.

## Acceptance criteria

- [ ] **S11-AC1:** Clean setup/build/test/package smoke succeeds on the declared matrix, with exact versions and artifact identities; all earlier gate evidence remains valid.
- [ ] **S11-AC2:** Required failure/recovery matrix passes with bounded outcomes; no unresolved critical lifetime, data-loss, privacy/security, or busy-loop defects.
- [ ] **S11-AC3:** Approved numerical performance/resource budgets are met by reproducible raw measurements, including upload/selection/network invariants. Unavailable metrics are disclosed and handled by an approved alternative, not fabricated zeros.
- [ ] **S11-AC4:** User docs accurately cover launch/control/config/scenes/privacy/troubleshooting/limitations; a new operator can follow them without undocumented global changes.
- [ ] **S11-AC5:** Software/artwork licenses and attribution are complete, artifacts are identifiable, and maintainer records M4 acceptance with date/reviewer and remaining non-blocking limitations.

## Failure and rollback

A failed budget or missing hardware result blocks release acceptance; open a roadblock with reproduction and next experiment. Changing limits/scope requires an approved change, never a post-hoc relaxed threshold. Roll back a regressing candidate to the last accepted artifact when available and mark affected gates stale. Do not publish, alter production/session configuration, or delete artifacts without explicit authorization.

## Handoff

Deliver a release decision packet: accepted gates, artifact IDs, matrix/budgets/results, known limitations, runbooks, and follow-up proposals. If not accepted, clearly state which gates remain open and the next owner/action; avoid calling it “done except for testing”.
