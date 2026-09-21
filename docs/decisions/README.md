# Decision register

Use [the ADR template](../templates/decision.md) for a durable technical decision. Status is `proposed`, `accepted`, `superseded`, or `rejected`; record actual approver/date and superseding links. An ADR does not replace a scope-change approval.

| Decision | State | Required by | Owner | Next action |
| --- | --- | --- | --- | --- |
| [ADR-0001 — Delivery baseline](ADR-0001-delivery-baseline.md) | accepted | S00 acceptance | `simplysoup` | Baseline ratified 2026-09-21; maintain stage-local decisions and evidence |
| Dependency/MSRV/backend matrix and raw-handle ownership strategy | open | S00 versions; S02 lifetime acceptance | S00/S02 assignee | Compile a minimal compatible dependency set; verify upstream APIs |
| Software/artwork licensing and fixture provenance | accepted | S00 acceptance | `simplysoup` | Code/docs use MIT; keep art assets and generated fixture packages out of the repository |
| PNG decoding, color path, size limits | open | S03 implementation | S03 assignee | Freeze version-1 validation and reference vectors |
| Animation authoring schema / GPU packing | open | S04 implementation | S04 assignee | Demonstrate all required effects and finite/representable parameters |
| Dawn/dusk, polar day/night, hemispheric seasons | open | S06 implementation | S06 assignee | Define boundaries and fixtures, including missing location |
| Weather provider, timeout, cache freshness/backoff, location consent | open | S07 implementation | S07 assignee + maintainer | Choose one provider and document mapping/terms/privacy |
| Power-event adapter and animation suspend timing | open | S08 implementation | S08 assignee | Verify actual signal delivery and missing-service behavior |
| Supported scaling/device matrix and resource budgets | open | S09 matrix; S11 benchmark start | maintainer | Approve named hardware, workloads, metrics, and thresholds |

“Open” rows are decision deadlines, not accepted designs or confirmed blockers. When work reaches a deadline without the decision, create a roadblock and stop the affected implementation. Number new ADRs sequentially; the integrator resolves concurrent ID collisions.
