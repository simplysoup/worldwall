# Roadmap-change flow — change the delivery baseline

Use when changing scope, order/dependencies, acceptance criteria, milestone boundaries, public semantics, supported platforms, resource budgets, or required architectural policy. Routine stage progress/owner/evidence updates do not need a proposal.

## Propose without rewriting the plan

1. Create `docs/changes/CR-NNNN-short-name.md` from the [template](../templates/roadmap-change.md); list it in the [register](../changes/README.md).
2. State the problem/evidence and exact before/after delta with requirement/stage/criterion IDs. Link a roadblock if one motivated the request; the two records have different jobs.
3. Compare doing nothing, the smallest correction, and the proposed alternative. Describe dependency, testing, resource, compatibility, documentation, and rollback consequences. Do not invent estimates; label assumptions.
4. Request a named maintainer decision. Until approval, keep the canonical contract/stages/roadmap unchanged and continue only work valid under both plans.

## Approve and apply

States: `proposed → approved → applied`; alternatives are `rejected` or `superseded`. Approval records actual identity/date and conditions; agent authors cannot impersonate a maintainer. A delegated worker cannot approve their own scope change.

On approval, the integrator updates the affected contract, stages, architecture/schema decisions, dependency ledger, status, and tests **in one coherent integration**. Record approval and application separately. An ADR supplies technical rationale but does not substitute for change approval.

If approval conditions are unmet, keep the proposal approved-but-not-applied and do not claim the new baseline. Increment the contract version for normative changes and reference the applied proposal. Retain superseded records/links rather than deleting history.

## Check after application

- [ ] Every changed requirement has updated stage acceptance and validation coverage.
- [ ] Dependencies remain acyclic and milestone scope is consistent.
- [ ] Previously accepted evidence is marked stale where behavior changed.
- [ ] Active roadblocks are resolved only if their new gate is actually satisfied or legitimately removed by this proposal.
- [ ] Status, roadmap, interface docs, and skills no longer instruct conflicting behavior.
- [ ] Docs/code checks pass and the proposal links the applying revision/evidence.

Changing a benchmark threshold after observing a failure is a baseline change, not a “measurement correction”. Keep original results and obtain approval before rerunning against the new threshold.
