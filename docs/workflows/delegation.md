# Delegation, integration, and handoff

## Prepare a bounded assignment

The integrator reads the target stage and assigns one outcome with [the task template](../templates/stage-task.md): stage/gate IDs, inputs, allowed files, forbidden/shared files, commands, evidence format, decision authority, and stop conditions. Name who will integrate and review the result. If ownership is unassigned, do not invent a person or approval.

Delegate stable seams: pure selector tests, fixture generation, schema validation, scheduler models, evidence review. Avoid concurrent edits to the same renderer lifetime boundary, roadmap, status, or shared interface. Ahead-of-stage work is preparation only and cannot bypass dependency acceptance.

## Worker execution

1. Verify task inputs and local state; do not assume the delegator's build or test host is present.
2. Read the target stage and matching skill, then only relevant references. Resolve skill paths relative to `SKILL.md`; run commands from the project root.
3. Work within assigned files. If an interface/requirement must change, stop that portion and return a precise decision request.
4. Preserve failed commands and report missing validation honestly. Do not self-accept gates or edit shared status/roadmap unless explicitly acting as the integrator.
5. Return a [handoff](../templates/handoff.md) under `docs/handoffs/` with files/artifacts, exact checks/results, assumptions, unresolved risks, and the next safe action.

## Integrator review

- Inspect actual diff/files, not only the worker summary. Resolve ID/file collisions without overwriting another worker's evidence.
- Verify code/interface compatibility and rerun relevant integration checks. A worker's unit pass is not a hardware acceptance pass.
- Consolidate evidence; update shared ledger/status once. Send stage acceptance to the named authority with all criteria visible.
- Link roadblocks/change proposals separately, and capture reusable failures/practices when supported.

## Resume from a handoff

Read the latest handoff and canonical status/roadmap, inspect the current files/revision, and confirm external state (running process, socket, session changes, temporary assets). A conversation summary does not roll back disk or a running compositor. Revalidate stale assumptions before continuing.

Never leave a background process, modified user session, temporary credential, or untracked artifact undocumented. Do not start live suspend/session-changing tests without operator authorization.
