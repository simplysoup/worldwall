# S06 — Time, astronomy, and seasons

## Purpose

Produce testable environmental snapshots without coupling civil time to animation. Requirements: WW-06, WW-08. Dependency: S05. State/owner: [roadmap](../roadmap.md).

## Entry conditions

S05 accepted. Approve dawn/dusk boundaries, polar cases, location/timezone inputs, and hemispheric season convention. Never infer a user's location from IP as an undocumented convenience.

## Deliverables

Injectable wall-clock/astronomy providers, explicit location configuration, normalized day/season classifications, an explicit renderer-facing civil-seconds value for optional resident palette timelines, and table-driven boundary tests. GPU animation keeps its own monotonic clock.

## Execution

1. Separate UTC storage/observation timestamps from local civil-time interpretation and monotonic durations. Define timezone source and config validation.
2. Represent sunrise/sunset as optional/explicit polar states rather than requiring impossible times every day. Missing location yields a documented unknown classification/fallback.
3. Implement the approved day/dawn/dusk/night and season boundaries, including southern hemisphere and equatorial/unknown policy.
4. Refresh snapshots on explicit selection events. Supply civil seconds to an already selected optional palette atlas on clock updates, but do not upload scene resources or invoke selection. Status may show current time, but midnight/sunrise does not invoke selection on its own.
5. Test fixed dates/coordinates/timezones, DST transitions, leap day, clock jumps, polar summer/winter, and missing/invalid coordinates.
6. Run end-to-end selection with fixture snapshots to prove that environment interpretation changes results only on a trigger.

## Acceptance criteria

- [ ] **S06-AC1:** Approved classification policy and reference fixtures cover boundaries, DST, leap day, hemispheres, polar states, and missing/invalid location.
- [ ] **S06-AC2:** Civil clock jumps do not corrupt monotonic frame scheduling/animation; no local-time arithmetic is used for duration deadlines.
- [ ] **S06-AC3:** Explicit refresh can select an appropriate fixture scene; simply crossing a day/season boundary may update only a resident optional palette uniform and does not change the active scene.
- [ ] **S06-AC4:** Location defaults and diagnostics avoid silent geolocation and do not expose precise coordinates in ordinary logs/status.

## Failure and rollback

Invalid time/location/astronomy results produce explicit unknown state and a valid fallback, not a crash or invented sunrise. Keep deterministic fixtures for algorithm/library changes; a different classification requires a decision and updated tests, not a hidden golden-file rewrite.

## Handoff

Supply classification ADR, provider interfaces, boundary vectors, privacy behavior, and trigger evidence. CR-0003's complete catalog/timeline fixture follow-up is recorded in the [CR-0003 evidence](../evidence/2026-09-22-cr003-scene-variants.md); it preserves the explicit civil-seconds input and no-reselection boundary. S07 consumes this snapshot model without making the renderer network-aware.
