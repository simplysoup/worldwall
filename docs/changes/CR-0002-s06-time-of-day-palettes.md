# CR-0002 — Add scene time-of-day palette timelines

- State: applied
- Proposer / date: Pi coding agent / 2026-09-22
- Decision owner: `simplysoup`
- Baseline version / affected IDs: delivery contract v0.2; S06 / S04 / WW-03 / WW-04 / WW-06 / WW-08; ADR-0004 and ADR-0005
- Motivation / evidence: the requested validation must run V19, V26, and V29 through their reference time-of-day palettes for every weather variant. The public reference scripts were inspected and stored outside the repository at `/tmp/reference-scenes/{V19,V26,V29}.json`. They contain 26/36/15 alternate 256-color palettes and civil-second timelines with 27/37/21 keyframes. The current scene contract has one static 256×1 palette and S06 intentionally exposes time only to selector classification, so it cannot represent or render these palettes.

## Exact delta

| Item | Before | Proposed after | Reason |
| --- | --- | --- | --- |
| Scene package assets | One required `palette.png` (256×1 RGBA8); optional animation metadata | Add optional `time_of_day.toml` plus a contained 256×N `time_of_day.png` RGBA8 palette atlas. The manifest remains an explicit opt-in resource and absent assets preserve current behavior. | Represent the reference's alternate palettes without replacing the authoritative base palette contract |
| Time-of-day schema | No palette timeline | `time_of_day.toml` v1 validates ordered civil-second keyframes, atlas rows, wrap at 86,400 seconds, and byte-space fade semantics. Adjacent keyframes are linearly faded per authored byte channel using the reference floor rule. | Match V19/V26/V29 reference behavior and make civil-time input explicit |
| Scene resources / renderer boundary | Renderer receives one palette and monotonic animation time; shader has no civil-time input | Scene upload includes immutable atlas/keyframe resources. Each frame writes the existing bounded monotonic animation time plus a civil-seconds value supplied by the environment owner. The shader samples/fades the two selected palette rows, then applies existing GPU animation effects. | Keep per-pixel work on the GPU and prevent weather/clock ticks from reselecting scenes |
| Environment ownership | S06 produces selector-facing classifications; no renderer time value | Add a renderer-facing local-civil-seconds value derived from the already explicit S06 wall-clock/offset inputs. Clock changes update the palette uniform only; selection remains trigger-bound. Test-only civil-time injection is allowed. | Make time-of-day rendering deterministic without IP location, telemetry, or hidden timezone lookup |
| V19/V26/V29 local fixtures | Base palette and BlendShift animation only | Add generated local untracked time-of-day atlases/manifests from the inspected reference scripts; retain source/license warnings and do not bundle artwork. | Enable the requested 10-second-per-variant sequence validation |

## Alternatives and impact

Doing nothing leaves the current one-palette renderer unable to reproduce the
reference's civil-time palette changes. Selecting a different scene for every
time or weather value would violate the accepted selector boundary and cause
scene churn. CPU palette regeneration per frame would violate WW-03 and the
S04 upload invariant.

The proposed smallest correction is an optional immutable palette atlas with a
small keyframe table. The renderer performs row selection/fading in WGSL and
continues to submit only bounded uniforms each frame. The atlas is uploaded
once with the scene; no CPU pixel or palette recoloring occurs during steady
state. Existing scenes without the optional asset remain static or use their
existing monotonic effects.

The change adds a scene resource and a civil-seconds uniform, so it amends the
S04 resource/schema decision and S06 renderer-facing time boundary. It must
not add environment-driven scene selection, weather-specific scene swaps, or
implicit location/timezone discovery. It requires parser tests, keyframe wrap
and byte-fade CPU vectors, GPU readback vectors, scene registry containment
checks, and a real assigned-session sequence with ten seconds per distinct
weather variant. The sequence must report scene upload count and prove that
weather labels do not change the selected scene unless an explicit selection
trigger is used.

Rollback is removal of the optional `time_of_day` manifest field and atlas
resource. Existing packages and the one-palette renderer path remain valid.
No real suspend/resume handling is included; that remains S08.

## Approval

- Decision / conditions: approved for implementation; preserve explicit civil-time inputs, keep the atlas optional, do not add environment-driven scene selection, and keep reference artwork untracked.
- Approver identity / date: `simplysoup` / 2026-09-22, recorded from the maintainer's explicit implementation instruction.

## Application

- Applying revision / evidence: working-tree implementation on 2026-09-22; [CR-0002 implementation update](../updates/2026-09-22-cr002-time-of-day.md); no Git commit is present in this checkout.
- Updated contract version / files: delivery contract v0.3; ADR-0004 and ADR-0005; scene format; S04/S06 stage docs; `src/scene.rs`, `src/scene_registry.rs`, `src/renderer.rs`, `src/wayland_surface.rs`; parser/pure/GPU tests; local untracked reference fixtures.
- Accepted evidence made stale: the earlier claim that S06 has no renderer-facing civil-time resource and the earlier S04 resource-layout description are superseded by the linked additive update. Existing selector-boundary evidence remains valid; the new live/package sequence gate is not accepted by this record.
- Linked roadblock disposition: none; this was an approved baseline change, not a technical failure.
