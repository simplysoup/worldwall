---
name: worldwall-stage-execution
description: Start or resume a Worldwall delivery stage or bounded implementation slice. Use before coding to verify dependencies, approvals, file ownership, acceptance evidence, and safe handoff conditions.
---

# Execute a Worldwall stage

## Read on demand

Read [status](../../../docs/status.md), [roadmap](../../../docs/roadmap.md), the target stage linked there, and its contract/decision references. Follow [development](../../../docs/workflows/development.md). Resolve these paths from this skill directory; run commands from the project root.

## Procedure

1. Inspect actual files and available tooling; if `.git` exists inspect the working tree. Do not assume Rust code or prior acceptance exists because a design document describes it.
2. Confirm stage dependencies, decision deadlines, maintainer approval, owner, and validation host. If missing prerequisites prevent assigned work, use the roadblock flow; do not guess approval.
3. Define one outcome with stage/requirement/criterion IDs, allowed files, baseline checks, and stop conditions. Use [task template](../../../docs/templates/stage-task.md) for nontrivial/delegated work.
4. Implement the smallest testable slice. Preserve environment/selection/render/presentation boundaries. Use renderer-review for raw handles, output state, shader resources, or scheduling.
5. Run relevant [checks](../../../docs/validation.md). Separate unit/headless/live results. A missing compositor is pending validation, not a pass.
6. Return files, exact results, artifact identity, limitations, and next safe action in evidence/update or [handoff](../../../docs/templates/handoff.md).
7. Ask the integrator to update the ledger/status; workers do not self-accept. Scope changes use roadmap-change, not a rewritten stage checklist.

## Stop and route

Unsafe ownership shortcuts, unapproved interface changes, repeated failed hypotheses, missing hardware/decisions, or out-of-scope files require a bounded escalation. Do not start weather work to evade an unresolved rendering gate. Do not create the entire final module tree before it is needed.

## Done

The assigned slice has evidence and a useful handoff. A stage is done only when every gate and dependency is reviewed/accepted by the assigned authority.
