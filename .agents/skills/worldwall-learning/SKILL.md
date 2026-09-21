---
name: worldwall-learning
description: Capture or revise Worldwall failure modes, regression guardrails, and best practices after a diagnosis, recovery, successful technique, or dependency change. Use to turn evidence into reusable guidance without presenting hypotheses as confirmed incidents.
---

# Capture reusable learning

Read [learning workflow](../../../docs/workflows/learning.md), [failure modes](../../../docs/knowledge/failure-modes.md), and [best practices](../../../docs/knowledge/best-practices.md).

1. Search existing mechanism/practice IDs before adding a duplicate. Expand an existing reserved ID when appropriate.
2. For a failure, separate trigger, symptom, suspected mechanism, and confirmed cause. Classify anticipated/suspected/reproduced/resolved-with-regression-test accurately.
3. For a practice, state when to use, concrete steps, why it helps, counterexamples/limits, and evidence. Keep recommended-unvalidated until demonstrated here.
4. Use [failure template](../../../docs/templates/failure-mode.md) or [practice template](../../../docs/templates/best-practice.md) for substantial detail; a short index row can suffice for an anticipated risk.
5. Link actual regression tests, roadblocks, decisions, and evidence. Proposed tests remain explicitly not run.
6. Add only a short relevant guardrail/link to the stage or skill; do not duplicate the incident log throughout the project. Normative scope/acceptance changes still require a CR.
7. Run `python3 tools/check_docs.py` from the project root and identify when to revalidate the guidance (API upgrade, recurrence, release).

## Output

A discoverable FM/BP entry with evidence status, prevention/recovery, limits, and maintenance trigger. Do not store secrets, exact user location, or unsupported performance claims in examples.
