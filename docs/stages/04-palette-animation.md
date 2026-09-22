# S04 — Palette animation and frame pacing

## Purpose

Animate on the GPU at a bounded resource cost. Requirements: WW-03, WW-04, WW-05, WW-11. Dependency: S03. State/owner: [roadmap](../roadmap.md).

## Entry conditions

S03 accepted. Approve exact version-1 animation authoring fields, units/ranges, GPU packing, effect definitions, and comparison tolerances in an ADR. The outline's four floats are not a complete effect specification.

## Deliverables

Animation loader/validation, immutable GPU metadata, optional immutable civil-time palette atlas/keyframes, bounded clock uniforms, hue/pulse/linear palette-cycle interpolation and authored-byte BlendShift effects with groups, callback-plus-deadline scheduler, and first measured resource baseline.

## Execution

1. Define static defaults and reference vectors for each required effect. Palette interpolation must address an explicit ordered group/range, not merely alter hue and call it cycling. BlendShift vectors must cover integer rotation, 1/100 fractional byte fades, and wrap direction.
2. Validate finite numbers, representability, group overlap/ranges, speeds, and amplitude bounds; convert authoring metadata once per load.
3. Implement shader effects in the approved color space. Keep civil-clock/weather/location policy absent from shader behavior; only the monotonic animation phase is a shader input. Bound animation phase precision for long runtimes.
4. Drive the monotonic animation clock and a small uniform update; keep all full scene/palette/metadata uploads out of the normal frame loop.
5. Combine callback readiness with a 30 FPS deadline; test 15 FPS mode. Do not catch up by submitting bursts after stalls. Handle withheld callbacks, bootstrap frames, and resize without deadlock.
6. Instrument submission timestamps, selection generation, full uploads, CPU cost, and memory. Capture static versus animated baseline using [validation](../validation.md).
7. Use fake-clock scheduler tests for early/late callbacks and jumps; live-test sustained animation and compositor occlusion behavior. Do not assume every compositor suppresses wallpaper callbacks when covered.

## Acceptance criteria

- [ ] **S04-AC1:** Static/hue/pulse/linear palette-cycle, authored-byte BlendShift, and the CR-0002 civil-time byte-fade interpolation/group fixtures match their reference behavior; malformed/overlapping/unrepresentable metadata is rejected.
- [ ] **S04-AC2:** At 15/30 FPS, timestamp evidence and fake-clock tests show callback AND deadline pacing, no busy spin, no catch-up burst, and a functioning bootstrap path.
- [ ] **S04-AC3:** Steady state updates only bounded frame data; upload counters/code review confirm no CPU pixel recoloring or full texture regeneration.
- [ ] **S04-AC4:** Long-running phase/wrap tests, delayed callbacks, resize, and paused/no-ready output paths remain responsive and visually valid.
- [ ] **S04-AC5:** Reproducible initial CPU/GPU/memory/pacing measurements are retained with limitations; they are a baseline, not yet S11 performance acceptance.

## Failure and rollback

Keep a static mode and the no-timeline path to isolate effects from presentation. If packing cannot represent the approved schema, stop and revise the layout decision rather than dropping effects silently. Timeline resources remain immutable and optional; a failed timeline load rejects only the candidate package. Timing bugs must be reduced with fake-clock traces before adjusting sleeps by trial and error.

## Handoff

Provide schema/layout ADR, shader vectors, scheduler state diagram/tests, counters, and raw benchmark artifacts. S05 must replace scenes without rebuilding this architecture or introducing CPU animation. CR-0003's complete reference inventory and local package derivation are recorded in the [follow-up evidence](../evidence/2026-09-22-cr003-scene-variants.md); it does not change the animation contract.
