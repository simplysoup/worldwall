# ADR-0001 — Delivery baseline

- **Status:** accepted
- **Date proposed:** 2026-09-21
- **Decision owner / approver:** GitHub account `simplysoup`
- **Acceptance date:** 2026-09-21
- **Affected stages:** S00–S11
- **Evidence:** [outline audit](../audit.md)

## Context

The original outline explains intended architecture but has no implementation or acceptance history. It repeats requirements, leaves critical interfaces ambiguous, and combines IPC, performance, recovery, and polish into one final phase.

## Proposed decision

Adopt [delivery contract v0.1](../delivery-contract.md) and the [stage roadmap](../roadmap.md) as the working delivery baseline. Preserve original phases as S01–S10, adding S00 foundation and S11 release acceptance. Define the static-image MVP at S03.

Preserve the core environment/selection/GPU/presentation separation, startup/resume/manual selection, 30 FPS default, indexed textures, and shared device/resources. Clarify callback request-before-commit, lifetime proof, color handling, bounded refresh, and transactional replacement.

Use a versioned `animation.toml` authoring format instead of an undefined float-in-PNG convention. The exact GPU layout remains an explicitly gated S04 decision, not an implicit claim that four channels encode every effect. Add `reload-config` to separate configuration reload from `reload` reselection. Make deterministic selection/fallback rules and privacy defaults explicit.

## Alternatives

1. Keep the monolithic outline and add checkboxes: lower initial documentation effort, but preserves duplicated policy and unclear gates.
2. Scaffold the full proposed architecture immediately: increases integration surface before proving raw-handle presentation.
3. Drop advanced palette effects to fit four float channels: simplifies packing but changes intended scope; not adopted without an approved change.

## Consequences

More explicit evidence is required, but developers can work on bounded stages and diagnose blockers without silently changing the plan. Numeric performance limits, library APIs, provider choice, and artwork rights still require real decisions/evidence.

## Approval record

The assigned maintainer/reviewer, GitHub account `simplysoup`, ratified this
baseline on 2026-09-21 without amendment. The delivery contract, stage roadmap,
scene-format direction, command semantics, and affected S00 records are
reconciled with this decision. This ratifies the baseline; it does not accept
S00 or any later stage.
