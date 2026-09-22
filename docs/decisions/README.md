# Decision register

Use [the ADR template](../templates/decision.md) for a durable technical decision. Status is `proposed`, `accepted`, `superseded`, or `rejected`; record actual approver/date and superseding links. An ADR does not replace a scope-change approval.

| Decision | State | Required by | Owner | Next action |
| --- | --- | --- | --- | --- |
| [ADR-0001 — Delivery baseline](ADR-0001-delivery-baseline.md) | accepted | S00 acceptance | `simplysoup` | Baseline ratified 2026-09-21; maintain stage-local decisions and evidence |
| Dependency/MSRV/backend matrix | open | S00 versions | S00 assignee | Compile a minimal compatible dependency set; verify upstream APIs |
| Software/artwork licensing and fixture provenance | accepted | S00 acceptance | `simplysoup` | Code/docs use MIT; keep art assets and generated fixture packages out of the repository |
| [ADR-0002 — S02 Wayland/wgpu surface ownership](ADR-0002-s02-wayland-wgpu-ownership.md) | accepted | S02 lifetime review | `simplysoup` | Accepted 2026-09-21; remaining live S02 gates stay in the stage evidence |
| [ADR-0003 — S03 scene decoding, limits, and color path](ADR-0003-s03-scene-decoding-color.md) | accepted | S03 implementation | `simplysoup` | Accepted 2026-09-21; use the recorded limits/path for S03 evidence |
| [ADR-0004 — S04 palette animation schema and GPU packing](ADR-0004-s04-animation.md) | accepted | S04 implementation | `simplysoup` | Accepted 2026-09-21; amended by CR-0001 and CR-0002 on 2026-09-22 |
| [ADR-0005 — S06 civil time, astronomy, and seasons policy](ADR-0005-s06-time-astronomy-policy.md) | accepted | S06 implementation | Pi coding agent | Accepted by `simplysoup` 2026-09-22; amended by CR-0002; see S06 evidence |
| [ADR-0006 — S07 Open-Meteo provider](ADR-0006-s07-open-meteo.md) | accepted | S07 implementation | Pi coding agent + `simplysoup` | Accepted 2026-09-22; implement explicit runtime configuration and retain no-live-request default |
| [ADR-0007 — S08 power-event adapter and paused animation clock](ADR-0007-s08-power-events.md) | accepted | S08 implementation | Pi coding agent | Accepted by `simplysoup` 2026-09-22; retain the recorded suspend/topology limitations |
| Supported scaling/device matrix and resource budgets | accepted | S09 matrix; S11 benchmark start | `simplysoup` | Approved 2026-09-22 in the [S11 matrix/budget proposal](../updates/2026-09-22-s11-performance-budget-proposal.md); retain raw results and disclose unavailable counters |

“Open” rows are decision deadlines, not accepted designs or confirmed blockers. When work reaches a deadline without the decision, create a roadblock and stop the affected implementation. Number new ADRs sequentially; the integrator resolves concurrent ID collisions.
