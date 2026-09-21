# Roadblock flow — diagnose and unblock

A roadblock means an assigned task/gate cannot proceed: technical failure, unavailable test environment, dependency, permission, or overdue decision. A risk is only a possible future failure. A failed test fixed within the same bounded increment may stay in evidence unless it reveals a reusable failure mode.

## Record

1. Preserve the smallest reproduction, exact command, error/log, expected versus observed result, and environment. Redact credentials/location; never lose the failing artifact while “cleaning up”.
2. Create `docs/roadblocks/RB-NNNN-short-name.md` using the [template](../templates/roadblock.md), and add it to the [register](../roadblocks/README.md).
3. Name the affected stage/acceptance IDs, owner, impact, confirmed facts, hypotheses, attempts/results, temporary workaround/risk, and next bounded experiment or decision request.
4. Specify a next-check date **or event** (e.g. test-host access granted). If the required gate truly cannot proceed, integrator marks that stage `blocked` and links the record in its evidence cell; independent work may continue with explicit boundaries.

## Investigate

Use one falsifiable hypothesis at a time. Keep a minimal static/layer-only/mock baseline for comparison. Do not spend repeated retries changing unrelated dependencies, timing, and ownership simultaneously. After two failed approaches, reassess with new evidence or escalate to the required owner.

Escalate immediately for unsafe lifetime violations, privacy/security exposure, destructive recovery, or architecture/scope changes. Missing equipment goes to the test-host owner; provider decisions go to the maintainer; upstream bugs get a minimal reproducer without promising an upstream fix date.

## Resolve or change direction

- `open → investigating → resolved` only when a fix/workaround satisfies the **unchanged** gate and verification evidence is linked.
- `deferred` is not resolved; acceptance remains blocked unless an approved change legitimately moves/removes the affected requirement.
- Reopen a recurring issue with new evidence; preserve earlier resolution history.
- If the remedy changes scope, sequence, budgets, or acceptance, link a separate [change proposal](roadmap-changes.md). Do not rewrite the roadmap while writing the diagnosis.
- On resolution, integrator restores stage state, reruns relevant tests, updates status, and captures a [failure mode or best practice](learning.md) when reusable.

A workaround needs limits, expiry/removal condition, risk owner, and tests. Leaking handles, disabling tests, assuming `'static`, retry storms, and calling untested behavior supported are not acceptable workarounds.
