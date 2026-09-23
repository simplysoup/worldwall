# Worldwall delivery contract

**Version:** 0.7 · **State:** approved · **Baseline date:** 2026-09-21 · **CR-0001 applied:** 2026-09-22 · **CR-0002 applied:** 2026-09-22 · **CR-0004 applied:** 2026-09-22 · **CR-0005 applied:** 2026-09-22 · **CR-0006 applied to policy:** 2026-09-23 (implementation/acceptance gates remain open) · **CR-0007 applied:** 2026-09-23 (implementation present; acceptance remains open)
**Accountable maintainer / acceptance authority:** GitHub account `simplysoup`; baseline ratified in ADR-0001 on 2026-09-21.  
**Target:** Linux desktop with Hyprland, Wayland layer-shell, and a supported Vulkan adapter. Library/compositor versions and the tested device matrix are established by evidence, not assumed here.

This is an engineering delivery agreement, not a claim of an existing product. “Must” defines an acceptance requirement once the baseline is approved. S00 may perform setup and feasibility work before ratification; feature work requires the applicable baseline/decisions to be approved.

## 1. Outcome and delivery boundaries

Deliver a long-running foreground wallpaper engine that renders indexed pixel art using shared GPU resources, animates palette effects without CPU pixel recoloring, and chooses scenes at explicit environment-refresh events. Normal desktop applications remain usable above an input-free background.

| Milestone | Required stages | Deliverable |
| --- | --- | --- |
| M0 — Buildable foundation | S00 | Approved baseline, ownership, reproducible build/test setup, safe fixture provenance |
| M1 — Static rendering MVP | S01–S03 | One-output, opaque indexed image, correct cover scaling, resize, clean teardown |
| M2 — Animated scene engine | S04–S05 | Bounded GPU palette animation, registry, safe scene replacement and deterministic selection |
| M3 — Environment-aware engine | S06–S08 | Time/astronomy, optional cached weather, suspend/resume-triggered selection |
| M4 — Release candidate | S09–S11 | Multi-output lifecycle, secure local controls, recovery, measured resource cost, packaging/docs |

No dates or percentage estimates are promised without an assigned team and measured execution data. A milestone is accepted only after all required stage gates and dependencies are accepted. CR-0004 records the approved S09-AC5 evidence deferral: the physical incompatible-output result is future work and remains `not-run`; this does not claim universal hardware compatibility.

## 2. Functional and safety requirements

| ID | Requirement | Acceptance stages |
| --- | --- | --- |
| WW-01 | SCTK owns the Wayland connection and layer-shell lifecycle; each output uses a Background layer, all anchors, no keyboard or pointer/touch input, and deliberate exclusive-zone behavior. | S01, S09 |
| WW-02 | wgpu presents through the same display and `wl_surface`. Wait for configure/ack and a valid nonzero physical extent. Wayland objects outlive the dependent GPU surface. Vulkan is the first supported backend. | S02, S09 |
| WW-03 | Preserve exact 8-bit indices in `R8Uint`; resolve palette colors in the shader. Optional immutable civil-time or solar-event palette atlases are also sampled and faded on the GPU. Normal rendering does not regenerate image data or upload full scene textures per frame. | S03, S04, S06 |
| WW-04 | Support hue cycling, brightness pulsing, bounded linear-RGB palette-cycle/interpolation effects, authored-byte BlendShift effects, and optional versioned civil-time or solar-event palette timelines with coherent groups/resources. One global `animation_speed_multiplier` scales every animated palette effect, defaults to `0.5`, and is configurable from `0.0` through authored speed `1.0`; rate changes preserve animation phase. CPU environment code resolves solar anchors to a bounded row-pair/fraction; the renderer samples the immutable atlas. Time-of-day palette sampling is renderer input; weather/selection logic remains outside the shader. | S04, S06, S10 |
| WW-05 | Default ceiling is 30 FPS, configurable to 15 FPS. Use monotonic scheduling and compositor readiness; no spin loop or per-millisecond polling. Each output is independently paced. | S04, S09, S11 |
| WW-06 | Full family selection occurs only on startup, one coalesced resume, or explicit `reload`, `scene <id>`, or `next`. With the valid schema-v3 CR-0006 opt-in, an accepted fresh weather result may replace only the precipitation variant inside the active family; weather cannot change families. Without opt-in, weather remains cache/status-only. Clock/solar ticks, `reload-config`, resize, and hotplug never select a scene. Failures retain the last valid scene or a validated offline fallback. | S05–S08, S10 |
| WW-07 | Weather is optional and disabled by default. Schema-v2 config may explicitly select Open-Meteo only when a complete location/timezone is configured; schema v3 adds the default-off adaptive-scene switch. Legacy environment inputs remain supported only without a selected config file. Poll no faster than approximately 15 minutes under normal operation, cache results, retain last known values on failure, and expose freshness. Only a fresh known result with valid CR-0006 opt-in may request a same-family precipitation variant; network waits never block Wayland dispatch or frames. | S07, S08, S10 |
| WW-08 | Use local civil time and location-aware astronomy/season rules with explicit unknown/polar states. Schema-v2 location requires explicit coordinates and an IANA timezone; no host timezone/location is inferred. A civil-seconds or solar-event sample may update the resident scene's optional palette timeline without changing family. When schema-v3 adaptive selection is validly enabled, startup/resume may choose a family from the latest available temperature using the approved bands and 2°C hysteresis; first offline start falls back to V29. Sleep pauses rendering; resume refreshes environment and initiates exactly one coalesced full selection. Missing power integration is visible, not a fabricated resume. | S06, S08 |
| WW-09 | Share one supported device/queue and selected-scene textures across outputs. Surfaces, dimensions, per-output uniform state, and pacing are output-local. Handle add/remove/scale/resize and zero outputs safely. | S09 |
| WW-10 | Provide bounded same-user Unix-socket controls: `status`, `reload`, `reload-config`, `scene <id>`, `next`, `weather`. Schema v1-v3 remain readable; schema v3 adds `[weather].adaptive_scenes = false` by default and requires enabled Open-Meteo plus a valid adaptive registry when true. Schema v4 adds top-level `animation_speed_multiplier` in `0.0..=1.0`; omitted values default to `0.5` in all readable schemas. Invalid input or failed reload must not partially mutate live state. A successful config reload invalidates queued adaptive scene candidates, applies a phase-continuous animation-rate change, and does not select. | S10 |
| WW-11 | Gracefully handle asset errors, surface loss/outdated/timeout, unavailable weather, missing resume service, and shutdown. Unrecoverable device/OOM errors exit or enter a documented bounded recovery path, never an infinite retry loop. | S02–S11 |
| WW-12 | Deliver reproducible setup/run/config/control/recovery documentation, lawful assets, a tested version matrix, and an approved repeatable performance report. Do not claim zero CPU or verified low power without measurements. | S00, S11 |

## 3. Event semantics

| Event | Refresh environment? | Reselect? | Resource action |
| --- | --- | --- | --- |
| Startup | Clock/astronomy and cached weather; bounded best-effort fetch if enabled | One full selection. With adaptive opt-in, choose family using available temperature/appearance; unknown first-start data selects V29. A later fresh result may change only the selected family's variant. | Upload selected scene, start outputs; family/band state stores only family and band |
| Weather timer / completion | Update weather cache | With a valid adaptive opt-in, a fresh known result may request only a precipitation variant in the current family; never a family change | Decode candidate off the Wayland thread and commit through the existing transactional scene-swap boundary; stale/error/unknown results retain the resident scene |
| Clock tick / sunrise boundary | May update status data and a resident civil/solar palette sample | No | Bounded uniform update only; no scene or atlas upload |
| Suspend | No network work required | No | Pause deadlines/submission |
| Resume | Refresh time/astronomy and perform bounded stale-weather attempt | One full selection per coalesced resume. Adaptive family choice uses available temperature with persisted family/band hysteresis; a later fresh result may update only its variant. | Replace only if scene/content changed; recover surfaces as needed |
| `reload` | Same refresh policy as resume | Once | Atomic scene replacement if changed |
| `scene <id>` | Not required | Explicit override for this selection interval | Validate then atomically replace |
| `next` | Current available environment snapshot | Another compatible scene if one exists | Otherwise successful no-op with reason |
| `reload-config` | Validate candidate config; adjust services/scheduling, animation multiplier, explicit location/timezone, and optional provider | No | Existing scene remains resident; invalidate queued adaptive candidates from the prior config; animation phase remains continuous; invalid config rejected; newly enabled provider may start one bounded async request after commit |
| Resize / hotplug | No | No | Output-local reconfiguration only |

`scene <id>` is not a permanent pin across future reload/resume events. A weather result updates cache only when adaptive selection is absent/disabled; with valid opt-in, only a fresh known result may request a same-family precipitation variant. Neither a weather temperature crossing nor a late result changes family. Successful config reload and newer explicit selection invalidate queued adaptive candidates without changing the active scene. Config changes that remove the active scene do not discard its resident resources; a future selection must have a valid fallback. Re-uploading identical scene assets on every resume is unnecessary. See the exact bands, fallback order, and registry metadata in [scene format](scene-format.md) and [CR-0006](changes/CR-0006-weather-adaptive-scene-selection.md).

## 4. Non-goals for this baseline

- X11/GTK/EGL wrapper windows, non-Linux ports, multiple independent GPU devices, or arbitrary compositor compatibility.
- Video playback, CPU-recolored frames, per-frame network activity, automatic cross-family weather-driven swaps, or weather-driven scene changes without explicit schema-v3 opt-in.
- Per-output scene choices, a GUI editor, automatic IP geolocation, cloud accounts, telemetry, or bundled unlicensed artwork.
- Scaling modes other than nearest-neighbor cover, systemd as a runtime prerequisite, and automatic battery-policy tuning.

These require an approved roadmap change if added. A user service may be documented in S11 only after session-environment handling is tested; `exec-once = worldwall` remains the simple launch path.

## 5. Acceptance and authority

- An implementer produces code, tests, and evidence tied to `Sxx-ACn` and `WW-xx` IDs.
- A reviewer checks safety and behavior. The assigned maintainer/integrator records gate acceptance; a delegated worker cannot self-accept a stage.
- A single maintainer may fill multiple roles, but the record must say so and document the review performed.
- Every required acceptance item must have a result, artifact/command, environment, and reviewer. `not run`, `blocked`, and `failed` are not passes.
- No critical safety defect, missing required hardware evidence, unresolved scope decision, or waived criterion counts as complete. A waiver that alters acceptance requires an approved change and a revised contract/stage.
- Stage states and owner assignments live in the [roadmap](roadmap.md). Evidence is retained in [evidence records](evidence/README.md).

## 6. Change and record policy

Use [development updates](workflows/development.md) for progress, [roadblocks](workflows/roadblocks.md) for inability to proceed, and [change proposals](workflows/roadmap-changes.md) for a changed plan. An ADR explains an architectural choice; it does not independently authorize changing scope. Approval identity/date and the affected files must be recorded, never inferred from silence.

Until S00 approval, explicitly mark proposals rather than presenting new decisions as implemented facts. [ADR-0001](decisions/ADR-0001-delivery-baseline.md) identifies the proposed differences from the original outline.
