# CR-0005 — File-configured location/weather and solar palette lighting

- State: **approved and add-on acceptance recorded by `simplysoup`; implementation/private config applied; technical evidence status recorded separately**
- Proposer / date: Pi coding agent / 2026-09-22
- Decision owner: `simplysoup`
- Baseline version / affected IDs: delivery contract v0.4 → v0.5; S04, S06, S07, S10; WW-03, WW-04, WW-06, WW-07, WW-08, WW-10, WW-12; CR-0002
- Motivation / evidence: the requested operator workflow is to configure location and optional weather in Worldwall's config file and have local sunrise/sunset affect scene-authored animated lighting. Today `src/config.rs` accepts only `fps` and `registry_dir`; `src/wayland_surface.rs` reads location, UTC offset, and weather from environment variables; `time_of_day.toml` v1 keyframes use fixed civil seconds. The accepted S06 snapshot classifies time from sunrise/sunset but does not drive palette lighting. See [S10 config reference](../ipc.md), [CR-0002](CR-0002-s06-time-of-day-palettes.md), and [ADR-0005](../decisions/ADR-0005-s06-time-astronomy-policy.md).

## Exact delta

| Item | Before | Approved after | Reason |
| --- | --- | --- | --- |
| Runtime config / S10, WW-10 | `WORLDWALL_CONFIG` accepts only `schema_version`, `fps`, and `registry_dir`; location/weather inputs are environment variables. | Keep schema v1 readable. Add schema v2 with an optional `[location]` table (`latitude`, `longitude`, IANA `timezone`) and optional `[weather]` table (`provider = "disabled"` or `"open-meteo"`). Weather defaults to disabled and reuses the configured location; enabling it without a complete valid location rejects the candidate. When `WORLDWALL_CONFIG` is set, the file is authoritative for FPS/registry/location/weather; legacy environment inputs remain supported only when no config file is selected. | Put privacy-sensitive inputs in one auditable file, support DST through an explicit named IANA zone, and never infer location or enable network access implicitly. |
| Solar lighting / S04, S06, WW-03/04/08 | Optional `time_of_day.toml` v1 keyframes are fixed civil seconds; astronomy only feeds selection snapshots. | Preserve v1 civil timelines exactly. Add opt-in schema v2 solar timelines that reference the existing immutable palette atlas. Each keyframe names `event = "sunrise"` or `"sunset"`, a signed `offset_minutes` in [-720, 720], and an atlas `row`. A keyframe's instant is the local date's computed solar event plus that offset. For the current UTC instant, gather anchors from the previous/current/next local dates, order them by instant, and interpolate between the surrounding rows, including the overnight wrap. Duplicate resolved instants or unavailable/polar events select the base palette for the solar-timeline component. | Make authored lighting follow actual local solar events as they move seasonally, while retaining exact legacy civil timelines and predictable fallback. |
| Renderer behavior / S04, WW-03/04 | Civil-time palette interpolation is computed from a small clock value; normal frames update only bounded uniforms. | CPU environment code resolves a bounded lower-row/upper-row/fraction sample from the authored solar anchors; WGSL samples the existing atlas and applies the current authored-byte floor fade before the existing palette effects. Per-frame data remains bounded; atlas/scene textures remain one-time uploads. Missing location/timezone or polar dates use the authoritative base palette for the timeline, while monotonic hue/pulse/cycle/BlendShift effects continue normally. | Keep per-pixel work and color interpolation on the GPU without uploading palettes or recoloring image pixels on the CPU. |
| Runtime behavior / S06, S07, WW-06/07/08 | Config reload affects FPS/registry only. Weather and civil inputs are startup environment values. Clock/weather results do not reselect. | `reload-config` validates and atomically applies timezone/location/provider changes. A newly enabled Open-Meteo provider may start one bounded asynchronous request through the existing worker; a disabled provider makes no requests. Clock/solar/weather updates may update resident palette inputs and cache/status only. Scene selection remains limited to existing explicit triggers. | Enable the requested file-based controls without weakening privacy, worker isolation, or event semantics. |
| Accepted gates / S04/S06/S07/S10 | Existing evidence covers v1 config, civil-time fades, solar classification, and environment-variable weather. | Add strict config parsing/atomic-reload/privacy tests; IANA-zone/DST conversion tests; solar-anchor CPU reference and GPU readback vectors; daily movement, midnight wrap, dawn/dusk, duplicate-anchor, polar/unknown fallback, and no-reselection tests; provider-offline and config-reload failure-path tests; and assigned-session visual/pacing/upload evidence. Keep prior acceptance historical and identify the new evidence separately. | The existing tests do not establish file-configured IANA inputs or solar-relative renderer behavior. |
| Local V29 demonstration | The local V29 test fixture has seven BlendShift groups at its source-derived speeds. | Make a private copy outside the provenance fixture and set each group speed to exactly 0.5× its current value. Do not modify or redistribute the source-derived fixture. | User-requested local animation-speed adjustment, kept separate from the canonical package and artwork provenance. |

### Approved config shape

```toml
schema_version = 2
fps = 30
registry_dir = "none"

[weather]
provider = "disabled" # use "open-meteo" only when explicitly opting in

# Optional; populate only with operator-supplied values:
# [location]
# latitude = <explicit decimal latitude>
# longitude = <explicit decimal longitude>
# timezone = "<explicit IANA timezone>"
```

The optional `location` table is all-or-nothing. The example intentionally supplies no coordinates or timezone; configure it only with operator-supplied values. Coordinates must be finite and in geographic bounds; `timezone` must be a recognized IANA name from the pinned timezone database. The optional weather table defaults to disabled if omitted. `provider = "open-meteo"` sends the configured coordinates to Open-Meteo over HTTPS at the existing bounded cadence. Configured coordinates/timezone are not written to status or ordinary logs. A supplied config file takes precedence over legacy environment equivalents; `WORLDWALL_SCENE_DIR` remains a separate scene-source input.

### Approved solar timeline shape and visual semantics

Existing schema v1 remains civil-second based. Solar scenes opt into schema v2:

```toml
schema_version = 2
basis = "solar"
atlas = "time_of_day.png"

[[keyframe]]
event = "sunrise"
offset_minutes = -30
row = 0

[[keyframe]]
event = "sunrise"
offset_minutes = 30
row = 1

[[keyframe]]
event = "sunset"
offset_minutes = -30
row = 2

[[keyframe]]
event = "sunset"
offset_minutes = 30
row = 3
```

These rows describe the palette before/after each event; the illustrative rows must be authored for the particular scene, not treated as universal day/night colors. At a given UTC instant the environment layer resolves the neighboring event-anchored atlas rows and interpolation fraction, using event anchors across adjacent local dates so the night wrap remains continuous as sunrise/sunset move. The shader performs the existing authored-byte floor interpolation from the immutable atlas, then applies the selected scene's normal monotonic animation effects. This is palette lighting, not a scene swap or a CPU-generated frame sequence.

If a solar event is unavailable (including polar day/night), location/timezone is missing, or two configured anchors resolve to the same instant, the solar timeline contributes the scene's base palette for that sample. The renderer does not invent solar events. Scenes with v1 civil timelines retain their current behavior. A clock/sunrise/sunset boundary changes only the resident palette sample; it never invokes scene selection or re-uploads the atlas.

## Alternatives and impact

- **Do nothing:** keep environment variables and v1 fixed-civil timelines. This avoids a baseline change but does not satisfy the requested config workflow or seasonal solar lighting.
- **Config-only increment:** move location/weather into a file but retain fixed-civil palette times. This does not make lighting follow the changing sunrise/sunset.
- **Approved increment:** extend the S10 config transaction, use a pinned IANA timezone database, feed local solar event anchors into the optional scene-authored palette atlas, and preserve S06/S07/S04 ownership boundaries. The config parser and timezone conversions gain pinned dependencies; exact locked versions are recorded in `Cargo.lock`. No location is inferred and no provider is contacted unless `open-meteo` is explicitly enabled.

This touches the accepted S04/S06 renderer/environment interface, S07 provider setup/reconfiguration, and S10 config semantics. It does not add weather-driven shaders, automatic reselection, per-frame CPU pixel work, telemetry, bundled art, or multiple independent GPU devices. Solar values stay out of the selector except for the already accepted environment snapshot used by explicit selection triggers. Weather completion remains cache-only.

Risks include stale bundled timezone rules until the dependency is updated, invalid IANA names, DST boundary conversion, coincident/reordered solar anchors, polar/no-event dates, and palettes whose authored rows do not visually suit their event anchors. Tests and a renderer ADR must cover these before the new behavior is claimed. The local V29 speed change remains in an external private copy only.

Rollback removes schema-v2 config fields and solar-timeline support, restores legacy environment inputs, and retains schema-v1 civil timelines. No prior stage is reaccepted merely by applying this proposal. The separate CR-0005 add-on acceptance record above does not rewrite the original S04/S06/S07/S10 evidence.

## Approval

- Decision / conditions: **approved** for implementation with an explicit IANA timezone, scene-authored solar-event palette transitions, the base-palette fallback above, no automatic selection on time/weather changes, and a local-only 0.5× V29 speed copy.
- Approver identity / date: `simplysoup` / 2026-09-22, as confirmed in the user's design direction.
- Add-on acceptance: `simplysoup` explicitly directed on 2026-09-22 that CR-0005 add-on approval marks its gates accepted. This is recorded as maintainer acceptance; it does not assert that unrun technical/live tests produced evidence or that a hardware test passed. See the [test-batch run record](../updates/2026-09-23-cr0005-test-batch-run-guide.md) for produced results and the remaining live-test limitation.

## Application

- Applying revision / evidence: applied in the working tree; see [CR-0005 implementation evidence](../updates/2026-09-22-cr0005-config-location-solar.md) and the [2026-09-23 test-batch/live-run record](../updates/2026-09-23-cr0005-test-batch-run-guide.md) for changes, actual test results, private V29 copy, runner recovery, assigned-session traces, and remaining evidence limits.
- Updated contract version / files: delivery contract v0.5; config, environment, solar timeline, renderer, cache, IPC/config, stage, architecture, operator, status, and register references reconciled. At the initial implementation update no operator location had been supplied; the subsequent private config application is recorded in the linked follow-up without copying precise coordinates into project documentation.
- Acceptance/evidence distinction: no original S04/S06/S07/S10 evidence was invalidated; prior stage acceptance remains historical. CR-0005 add-on acceptance is recorded by the authorized maintainer as directed. The 2026-09-23 offline batch supplies CPU/config/fake-provider and offscreen Vulkan results. The corrected assigned-session run supplies config-reload/upload traces; the user reported all-purple colors on the synthetic test atlas, then reported that the source-preserving V29 display looked good during a later run that was aborted before finalization. Output 115 has sparse frame records requiring review. The source V29 time-of-day atlas remains unchanged. Acceptance does not transform those limitations into passing visual or pacing evidence.
- Linked roadblock disposition: none recorded. The live session test ran with an authored synthetic solar atlas, but the synthetic test-atlas visual result is `fail`, while the user reported the source-preserving V29 display looked good in an incomplete later run. The live-test fixture now preserves the original Effect Games time-of-day timeline and atlas; output-115 pacing remains unresolved. Do not treat headless readback, aggregate pacing statistics, or process exit status as a substitute for reviewing those results.
