# Roadblock register

Follow the [roadblock workflow](../workflows/roadblocks.md) and copy the [template](../templates/roadblock.md) to `RB-NNNN-short-name.md` here. Keep resolved rows with evidence links. Do not claim a hypothetical bug was reproduced, or resolve a blocker just by deferring its acceptance criterion.

| ID / record | Stage / gate | State | Owner | Next check / action |
| --- | --- | --- | --- | --- |
| [RB-0001 — S11 benchmark matrix and budgets](RB-0001-s11-benchmark-budgets.md) | S11 entry / S11-AC3 | resolved | Pi coding agent → `simplysoup` | Resolved by matrix/budget approval plus the maintainer result review in the S11 disposition update |
| [RB-0002 — GPU utilization counter](RB-0002-gpu-utilization-budget.md) | S11-AC3 | resolved | Pi coding agent → `simplysoup` | Resolved by pmon-backed reruns in the S11 benchmark update |
| [RB-0003 — zero-output fallback](RB-0003-zero-output-fallback.md) | S11-AC3 B9 | resolved | Pi coding agent → `simplysoup` | Resolved with the approved B9 `not-run` host limitation; S11-AC3 criterion unchanged |
| [RB-0004 — CR-0006 adaptive artwork clarification](RB-0004-cr0006-adaptive-artwork-clarification.md) | CR-0006 artwork input | resolved (clarification and private crop validation; adaptive acceptance open) | Pi coding agent → `simplysoup` | Crop is visually reviewed and loader-validated in a private ordinary registry; adaptive/session gates remain open |
| [RB-0005 — scene loading blocks Wayland owner](RB-0005-scene-loading-blocks-wayland-owner.md) | S05-AC4 / S10-AC8 async path | resolved (offline implementation/tests; stage acceptance remains open) | Pi coding agent → `simplysoup` | Review bounded workers and FIFO final-response behavior; assigned-session evidence is a separate follow-up |

Pending decisions remain tracked in the [decision register](../decisions/README.md); anticipated technical risks are in [failure modes](../knowledge/failure-modes.md). Number sequentially; the integrator arbitrates concurrent IDs.
