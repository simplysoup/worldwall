---
name: worldwall-delegation
description: Assign, return, integrate, or resume bounded Worldwall work across developers or agents. Use when splitting tasks, preventing overlapping edits, reviewing delegated results, or restoring work from a durable handoff.
---

# Delegate and integrate safely

Read [delegation flow](../../../docs/workflows/delegation.md), the target stage, and current [roadmap](../../../docs/roadmap.md). Use [task](../../../docs/templates/stage-task.md) and [handoff](../../../docs/templates/handoff.md) templates.

## Assign

1. Verify dependencies and frozen interfaces. Split by stable seams such as fixtures, pure selector tests, or schema validation rather than multiple writers to one unsafe boundary.
2. Specify one outcome, criterion IDs, inputs, allowed/forbidden files, tests/host, integrator, reviewer, and stop conditions.
3. Reserve a single writer for roadmap/status/shared interface changes. Ahead-of-stage preparation does not accept future stages.

## Execute / return

4. Worker verifies local files/tools and reads only relevant stage/skill references. Stop for unapproved interface/scope changes; preserve failed experiments.
5. Return changed files/revision, exact checks/results, missing validation, decisions/assumptions, external side effects, blockers, and immediate next action. Do not self-accept gates.

## Integrate / resume

6. Inspect actual changes and rerun applicable integration checks; do not trust a summary as proof. Resolve record-ID conflicts without overwriting evidence.
7. Integrator consolidates evidence and updates shared ledger/status after review; maintainer accepts gates.
8. On resume, inspect disk/process/socket/session state. A conversation summary does not revert external state. Stop or clean up only task-owned resources with appropriate authorization.

## Output

A bounded assignment or durable handoff with sufficient state for the next developer, no hidden processes/session changes, and no ambiguous shared-file ownership.
