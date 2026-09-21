# Failure modes and best practices

Capture reusable knowledge after a meaningful diagnosis/recovery/review. Do not turn every speculative concern into a confirmed incident or every preference into a mandatory rule.

## Failure modes

1. Check the [failure-mode index](../knowledge/failure-modes.md) for an existing mechanism before adding an entry.
2. Use `docs/knowledge/failures/FM-NNNN-short-name.md` and the [template](../templates/failure-mode.md) for substantial detail; concise anticipated risks can remain index rows.
3. Classify as `anticipated`, `suspected`, `reproduced`, or `resolved-with-regression-test`. Separate symptom, mechanism, and confirmed root cause.
4. Include trigger, affected boundary, minimal reproduction/evidence, detection, recovery, prevention, regression test, owner, and related stage/roadblock/decision.
5. Link the actual test once implemented. A proposed test path is not a passing regression test. Never put sensitive raw provider/session data in the catalogue.

## Best practices

1. Add or refine [the best-practice index](../knowledge/best-practices.md); use `docs/knowledge/practices/BP-NNNN-short-name.md` and [the template](../templates/best-practice.md) when a worked example is useful.
2. State when the technique applies, its rationale, concrete steps, counterexamples, limits, and evidence.
3. Label it `recommended-unvalidated` until demonstrated in this project, then `validated` with linked evidence. Retire practices invalidated by dependency/API changes.
4. Promote a practice to a normative contract rule only through a roadmap change if it changes scope/acceptance. Update the relevant skill with a short link/guardrail, not a duplicate essay.

## Close the loop

A resolved roadblock can yield both a failure mode (what breaks and how to detect it) and a best practice (what reliably prevents it). Link them; do not copy the entire investigation. Recheck related guidance on dependency upgrades, repeated failures, or release review. Keep high-signal local knowledge close to the stage that uses it.
