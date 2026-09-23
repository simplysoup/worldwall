# CR-0007 — Global animation-speed multiplier

- State: applied to contract v0.7; implementation is present, acceptance remains open
- Proposer / approval date: `simplysoup` / 2026-09-23
- Decision owner: `simplysoup`
- Baseline before change: contract v0.6
- Affected requirements/gates: WW-04/10; S04-AC9; S10-AC9
- Motivation/evidence: maintainer direction in the 2026-09-23 continuation to apply the site's roughly half-speed default to every animation effect and expose a scalar config control; [RB-0004](../roadblocks/RB-0004-cr0006-adaptive-artwork-clarification.md)

## Exact delta

| Item | Contract v0.6 | Contract v0.7 | Reason |
| --- | --- | --- | --- |
| WW-04 / S04 | Animation effects use authored speeds. | One global dimensionless multiplier scales every animated palette effect (including BlendShift, hue cycle, brightness pulse, and palette cycle); the default is `0.5`. The existing monotonic animation phase is rate-integrated, so config changes preserve phase and do not rewind it. Civil/solar palette sampling remains on its separate civil-time input. | Apply the requested approximate half-speed behavior uniformly rather than editing individual scene effects. |
| WW-10 / S10 | Config schemas through v3 contain FPS, explicit location/weather, and default-off adaptive selection. | Schema v4 adds the top-level scalar `animation_speed_multiplier`, accepted from `0.0` through `1.0`; absent values in schema v1-v3 and the environment-backed config default to `0.5`. Reload applies a validated rate change without scene reselection. | Preserve old config readability while exposing an explicit override; `1.0` is authored speed and `0.0` pauses palette animation. |
| V29 private copy | The local runtime copy contained individually halved BlendShift speeds. | The private copy now matches its source animation metadata; the global default produces the same effective half speed. The previous local manifest is preserved in private state. | Avoid applying the global factor twice and make V29 consistent with other scenes. |

## Alternatives and impact

- **Do nothing:** retain per-package authored rates, including the previous V29-only half-speed edit; this contradicts the requested uniform behavior.
- **Edit every scene manifest:** duplicates policy across assets, risks source/provenance drift, and makes later user changes cumbersome.
- **Approved implementation:** keep authored animation data unchanged and scale the bounded monotonic GPU animation phase centrally. The scalar does not change pixel work or GPU resource allocation. Config reload changes the integrated rate continuously and leaves scene/selection generations untouched.

The conservative range is `0.0..=1.0`: it allows pausing, slowing, and restoring authored speed without running faster than source authoring. Animation metadata validation and shader layout remain unchanged. Older schemas stay readable and receive the new default; users requiring an explicit override use schema v4. Rollback is to set the multiplier to `1.0`; retain the field and old evidence. The existing private V29 folder name still says `halved`, but its animation manifest is source-speed and the runtime multiplier supplies the half-speed behavior.

## Approval

- Decision / conditions: `simplysoup` directly instructed the global half-speed behavior and configurable scalar on 2026-09-23. The schema-v4 key, conservative `0.0..=1.0` range, and phase-continuous reload behavior are the implementation of that direction.
- Approver identity / date: `simplysoup` / 2026-09-23.

## Application

- Applying baseline: delivery contract v0.7, 2026-09-23.
- Updated code/docs: config schema/parser and validation, phase clock, multi-output runtime, example config, WW-04/10 and S04/S10 criteria, architecture/scene/IPC/README, roadmap/status, and change register.
- Offline implementation checks: config range/default/legacy-compatibility tests and deterministic multiplier/pause/reload phase tests pass in `cargo test --offline --all-targets --all-features` (109 main tests passed; 3 Vulkan readback tests ignored; 4 CLI tests passed).
- Private asset migration: previous V29 halved manifest is retained in a private state backup outside the repository; the runtime copy now has the source manifest. The source fixture was not changed. The Monolith private review crop is tracked in RB-0004; no artwork is added to this repository.
- Acceptance is not claimed. Assigned-session animation visual/pacing checks, live config reload, adaptive swap, and next-login behavior remain not run. CR-0006 async scene/config loading remains blocked by [RB-0005](../roadblocks/RB-0005-scene-loading-blocks-wayland-owner.md). No Worldwall process was launched and no private operator config was changed.
