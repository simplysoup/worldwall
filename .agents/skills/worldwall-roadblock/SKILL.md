---
name: worldwall-roadblock
description: Diagnose and document a Worldwall task that cannot proceed because of a technical failure, missing test host, dependency, permission, or overdue decision. Use after repeated failed experiments or a blocked acceptance gate, without silently changing the roadmap.
---

# Record and unblock a roadblock

Read [roadblock flow](../../../docs/workflows/roadblocks.md), [register](../../../docs/roadblocks/README.md), and the affected stage. Resolve links from this directory; commands run at the project root.

1. Preserve exact failing command/procedure, minimal input, expected/observed behavior, versions, and redacted logs before modifying the experiment.
2. Distinguish confirmed facts from hypotheses and future risks. An unimplemented future feature is not an observed defect.
3. Create or update an RB record with [the template](../../../docs/templates/roadblock.md): stage/gates, owner, impact, attempts/results, safe workaround/risk/expiry, next bounded experiment, and next-check date/event.
4. Use one falsifiable hypothesis per experiment. After two failed approaches to the same hypothesis, reassess or escalate instead of repeating blind retries.
5. Have the integrator mark an actually blocked stage and link the record; allow only explicitly independent work to continue.
6. If the remedy changes scope, ordering, acceptance, or budgets, create a linked CR using roadmap-change. Leave the baseline unchanged until approval.
7. Resolve only with verification satisfying the current gate; link evidence, restore appropriate stage state, and capture reusable learning if warranted.

## Safety and output

Escalate lifetime violations, security/privacy exposure, destructive recovery, and missing authority immediately. Never “fix” a gate by disabling tests, leaking owners, auto-geolocating, or claiming untested hardware support.

Return the blocker ID, current confirmed diagnosis, next experiment/decision owner, and safe parallel work. Deferred is not resolved.
