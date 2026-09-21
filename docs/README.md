# Documentation map

## Sources of truth

| Question | Canonical source |
| --- | --- |
| What must be delivered? | [Delivery contract](delivery-contract.md) |
| What is next, owned, blocked, or accepted? | [Roadmap](roadmap.md) |
| What was actually verified and what should happen now? | [Status](status.md) |
| How is a stage implemented and accepted? | [Stage files](stages/) linked from the roadmap |
| Who owns resources and interfaces? | [Architecture](architecture.md) |
| What do scene files mean? | [Scene format](scene-format.md) |
| What checks count? | [Validation](validation.md) and [evidence](evidence/README.md) |
| Why was a decision made? | [Decisions](decisions/README.md) |
| Why is work unable to proceed? | [Roadblocks](roadblocks/README.md) |
| Should the plan change? | [Change proposals](changes/README.md) |
| What can prevent repeat mistakes? | [Failure modes](knowledge/failure-modes.md) and [best practices](knowledge/best-practices.md) |
| What happened in an increment? | [Updates](updates/README.md) and [handoffs](handoffs/README.md) |

## Operating procedures

- [Development and progress updates](workflows/development.md)
- [Roadblocks](workflows/roadblocks.md)
- [Roadmap changes](workflows/roadmap-changes.md)
- [Failure modes and best practices](workflows/learning.md)
- [Delegation and integration](workflows/delegation.md)
- [Copyable templates](templates/README.md)
- [On-demand local skills](../.agents/skills/README.md)

## Authority and maintenance

The contract is the **approved v0.1 baseline**, ratified through ADR-0001 on 2026-09-21. Precedence is: approved contract → approved decisions and interface specifications → stage acceptance → roadmap scheduling → status summaries. If two normative documents disagree, stop the affected change, record the inconsistency, and obtain a reconciled decision; do not silently choose the easier requirement.

An approved change must update all affected normative documents in the same integration. Newer timestamps alone do not override requirements. The archive is never normative.

Keep implementation state only in the roadmap ledger. Stage files are execution specifications, not duplicate progress trackers. Status provides a short evidence-based interpretation and links to the ledger. Updates and evidence are append-only history; corrections should be explicit. Small changes may share one update/evidence record rather than creating a document per command.

## Original review

- [Audit and recommendation](audit.md)
- [Immutable original outline](archive/project-outline.original.md)
- [Baseline proposal](decisions/ADR-0001-delivery-baseline.md)
