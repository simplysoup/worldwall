# Durable handoffs

No delegated implementation handoffs have been produced yet. Use `YYYY-MM-DD-Sxx-short-name.md` here with [the handoff template](../templates/handoff.md) when a worker returns work or an incomplete task changes owner/context.

A handoff must restore intent, stage/gates, file/revision identity, checks/results, external side effects, decisions/assumptions, blockers, and next safe action. Follow [delegation](../workflows/delegation.md). Link a small completed change's update instead of duplicating it if all necessary fields are present.
