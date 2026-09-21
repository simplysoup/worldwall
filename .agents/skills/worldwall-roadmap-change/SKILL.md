---
name: worldwall-roadmap-change
description: Propose or apply an approved change to Worldwall scope, stage order, dependencies, acceptance criteria, public semantics, supported platforms, or resource budgets. Use for baseline changes, not routine progress or roadblock logging.
---

# Change the roadmap deliberately

Read [change flow](../../../docs/workflows/roadmap-changes.md), [contract](../../../docs/delivery-contract.md), [roadmap](../../../docs/roadmap.md), and [change register](../../../docs/changes/README.md).

1. Identify the exact baseline delta and affected WW/Sxx/AC IDs. If only owner/state/evidence changes, use progress-update instead.
2. Create a CR with [the template](../../../docs/templates/roadmap-change.md), evidence, before/after, alternatives, dependency/test/resource/compatibility impact, assumptions, and rollback.
3. Link motivating roadblocks and technical ADRs without duplicating their narratives. A diagnosis or ADR does not authorize scope changes.
4. Request explicit maintainer approval; keep normative contract/stages/roadmap unchanged while pending. Do not supply invented approval details or infer approval from silence.
5. After approval, the integrator applies all affected contract, stage, interface, ledger/status, test, and skill updates coherently. Record applying revision/evidence and baseline version; mark old evidence stale as needed.
6. Run docs/code checks from the project root. Verify dependencies remain acyclic and all revised requirements still have acceptance coverage.

## Output

Either a clearly pending/rejected/superseded proposal with next decision owner, or an approved-and-applied change with evidence and reconciled documentation. Never present an approved-but-unapplied proposal as the current implementation.
