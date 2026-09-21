---
name: worldwall-progress-update
description: Update Worldwall progress after an implementation increment, documentation change, validation run, or gate review. Use to keep evidence, stage state, and status consistent without changing roadmap scope.
---

# Update progress without changing the plan

Follow [development updates](../../../docs/workflows/development.md). Read the target stage, [roadmap](../../../docs/roadmap.md), and [status](../../../docs/status.md). Commands run from the project root.

1. Inspect artifacts/results, not just a previous agent's summary. Classify work as proposed, implemented, verified, or accepted; these are different states.
2. Write one concise update/evidence record: date, revision/hash, IDs, changed files, exact commands/observations, pass/fail/blocked/not-run, limitations, and next owner/action. Use [evidence template](../../../docs/templates/evidence.md); do not create unnecessary duplicate reports.
3. Check every acceptance criterion and dependency before suggesting `review` or `accepted`. Only the assigned maintainer records acceptance with reviewer/date.
4. If acting as integrator, update only state/owner/evidence in the roadmap ledger and refresh the short status summary. Otherwise return the proposed update for integration.
5. Link blockers and proposals separately. Invalidate stale accepted evidence explicitly after a regression; preserve its historical record.
6. Run `python3 tools/check_docs.py`; for checker changes also run `python3 -m unittest discover -s tools/tests -v`. Run applicable code checks only when their prerequisites exist.

## Guardrails

- Documentation and unit tests cannot prove live Wayland behavior or power savings.
- `blocked`, `not-run`, and unavailable metrics are never passing evidence.
- Changing scope/order/gates/budgets requires the roadmap-change skill; changing ledger state does not.
- Do not invent percent complete, approval identities, dates, host availability, or test outputs.

## Output

A truthful short progress summary, linked evidence, current ledger/status, and one next action. Unfinished work has a named owner or explicitly unassigned responsibility.
