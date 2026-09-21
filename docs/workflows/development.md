# Development and progress updates

Use this flow for execution **within the approved baseline**. Scope/sequence/gate changes go through [roadmap changes](roadmap-changes.md); inability to proceed goes through [roadblocks](roadblocks.md).

## Start an increment

1. Inspect current files/working tree. Read status → roadmap → target stage → required decisions. Avoid rereading the entire archive.
2. Confirm dependencies/approvals, assign one owner, and move the stage to `in-progress` when work starts. Do not start an unapproved feature because a stage is merely listed as `ready`.
3. Define one bounded outcome, affected acceptance IDs, allowed files, tests, and stop conditions. Use the [stage-task template](../templates/stage-task.md) for delegated/nontrivial work; a small local fix can keep this in its update record.
4. Take a baseline result before changing behavior. Unknown APIs require a small reproducible spike, not a broad guessed implementation.

## Execute and validate

- Implement the smallest end-to-end slice; add tests before expanding abstractions.
- Check relevant [validation layers](../validation.md). Explicitly label simulated, headless, and live results.
- For a meaningful failure, preserve command/log/environment before experimenting. After two failed approaches to the same hypothesis, reassess and record a roadblock or a new falsifiable hypothesis; do not retry blindly.
- Keep all work inside the assigned boundaries. If a shared interface or acceptance condition must change, pause that portion and request a decision.

## Close an increment

1. Save one concise evidence/update record with changed files, requirement/gate IDs, commands/results, artifact identities, and limitations. A single update can embed evidence if complete; do not create one file per test invocation.
2. Link any roadblock, decision, change proposal, failure mode, or best practice only if warranted. Routine fixes do not require a five-document ceremony.
3. Integrator updates the roadmap's owner/state/evidence cells only. Move to `review` when all gate evidence is present; maintainer moves to `accepted` with reviewer/date recorded in evidence.
4. Refresh status with actual progress, next action, and unresolved gates. Do not copy full stage plans or turn attempted work into completed features.
5. Run documentation checks and applicable code tests. Leave a handoff when work is incomplete/delegated or context is changing; a short completed local fix can use the same update record.

## Consistency checklist

- [ ] Roadmap state matches evidence, dependencies, approval, and blockers.
- [ ] No acceptance criterion was weakened to make a test pass.
- [ ] Missing live evidence is `blocked`/`not-run`, not `pass`.
- [ ] Status agrees with the ledger and identifies the next owner/action.
- [ ] Canonical interfaces changed with tests/docs; historical records are not silently rewritten.
- [ ] No duplicated long narrative in roadmap, status, and updates.

For a documentation-only correction, run docs checks, note the change briefly, and avoid inventing runtime progress. For an invalidated accepted gate, append a correction and mark downstream evidence stale until revalidated.
