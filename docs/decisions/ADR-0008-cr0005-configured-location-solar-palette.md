# ADR-0008 — Configured IANA location and solar palette sampling

- Status: accepted under [CR-0005](../changes/CR-0005-configured-location-weather-solar-lighting.md)
- Owner / date: Pi coding agent / 2026-09-22
- Approval identity / date: `simplysoup` / 2026-09-22 (CR-0005 approval)
- Affected stages / requirements: S04, S06, S07, S10; WW-03, WW-04, WW-06, WW-07, WW-08, WW-10
- Related change proposal: CR-0005

## Context and constraints

CR-0005 moves explicit location/timezone and optional weather into
`WORLDWALL_CONFIG`, and extends scene-authored palette lighting from fixed
civil seconds to local sunrise/sunset anchors. Existing schema-v1 configuration
and `time_of_day.toml` civil timelines remain supported. A selected config file
is authoritative; no location is inferred, and Open-Meteo remains disabled
unless the file explicitly selects it.

The architecture boundaries remain environment → selector → immutable scene
resources → renderer. Civil/solar/weather updates may change only bounded
resident frame inputs and cache state. They never select a scene or upload an
atlas again. Per-pixel palette work remains on WGSL/the GPU.

## Decision

### Runtime configuration and timezone

- `AppConfig` reads schema v1 (`schema_version`, `fps`, `registry_dir`) and
  schema v2. Schema v2 accepts an optional all-or-nothing `[location]` table
  with finite bounded latitude/longitude and a recognized IANA `timezone`, and
  an optional `[weather]` table with `provider = "disabled" | "open-meteo"`.
- Weather defaults to disabled. Open-Meteo requires the complete location and
  reuses it. When `WORLDWALL_CONFIG` is set, legacy environment values do not
  override file values. Legacy explicit environment inputs remain available
  only when no config file is selected.
- IANA rules come from pinned `chrono = 0.4.45` and `chrono-tz = 0.10.4`.
  The configured name is parsed before startup/reload commits; host timezone
  state is not consulted. UTC instants remain separate from monotonic frame
  deadlines.
- `reload-config` prepares and validates registry, timezone/location, provider,
  and scheduling replacements before commit. It does not select a scene. A
  changed weather provider may begin one bounded asynchronous startup request
  only after the old service is retired. A disabled provider starts no request.
- Weather cache schema v2 may carry a non-location config-generation scope
  derived from the selected file's modification time, size, and (on Unix) file
  identity. A scope mismatch is a cache miss, preventing stale weather from
  crossing location/provider edits without storing raw coordinates. Legacy
  schema-v1 unscoped cache files remain readable only by the unscoped legacy
  runtime.

### Solar timeline schema and resolution

- `time_of_day.toml` v1 remains a civil-seconds timeline with its existing
  wrap/fade behavior. V2 requires `basis = "solar"`; each keyframe uses
  `event = "sunrise" | "sunset"`, `offset_minutes` in [-720, 720], and an
  atlas `row`. At least two and at most 256 keyframes are accepted.
- The environment layer computes NOAA events for previous/current/next local
  dates using the explicitly configured location and timezone-derived local
  date. Event instants remain UTC and retain adjacent-day displacement across
  UTC midnight.
- CPU resolution sorts the bounded authored anchors for those dates and
  returns only lower row, upper row, and a clamped fraction. Missing location or
  timezone, unavailable/polar events, duplicate resolved instants, or an
  unbracketed sample selects the scene's base palette for the solar timeline.
  No event is invented.
- The renderer writes that bounded result into its existing per-output frame
  uniform. WGSL samples the existing immutable atlas and applies the authored
  byte-floor interpolation before ordinary palette effects. Solar fallback
  selects the base palette while monotonic hue/pulse/cycle/BlendShift effects
  continue. No palette/image pixels are generated or recolored on the CPU.
- Civil timelines do not consume solar inputs. Both timeline modes remain
  scene-authored and opt-in; a scene without an atlas uses its base palette.

## Alternatives

1. Keep only fixed civil timelines and environment variables: rejected by
   CR-0005 because it does not provide the approved config workflow or
   seasonally moving sunrise/sunset anchors.
2. Infer timezone/location from the host or network: rejected because inputs
   must remain explicit and auditable.
3. Resolve every pixel or regenerate palettes on the CPU: rejected because it
   violates the existing immutable-resource/GPU boundary.
4. Persist raw coordinates or include them in ordinary diagnostics/cache data:
   rejected; the cache uses a config-file generation scope instead.

## Consequences and validation

Schema-v1 files remain readable. Schema-v2 configuration and solar manifests
are strict and reject unknown/partial values. Unit tests cover IANA parsing,
DST changes, local-date rollover across UTC midnight, solar offsets, midnight
wrap, duplicate/polar/unavailable fallback, cache-scope changes, and
transactional configuration failure. The offscreen Vulkan readback test checks
solar row-pair/fraction interpolation and base-palette fallback.

These tests do not by themselves establish assigned-session visual quality,
steady pacing on every output, or output lifecycle. `simplysoup` explicitly
recorded CR-0005 add-on acceptance on 2026-09-22; that acceptance is separate
from the technical results. The corrected 2026-09-23 assigned-session run
passed config-reload and stable-upload checks, and recorded per-output frame
observations. Output 114 tracked its configured 30/15 FPS intervals; output
115 produced only 27/6 observations with long intervals, so multi-output
pacing remains under review. The user reported that the synthetic test scene
looked purple/all wrong; the test atlas had replaced the source Effect Games
colors only in the temporary bundle. This is a visual failure of that synthetic
fixture, not a color assessment of the unchanged V29 source palette. In a later
source-preserving display, the user reported that the V29 colors looked good;
the command was aborted before technical finalization. The run did not test
output lifecycle transitions; future V29 live fixtures preserve the original
civil-time atlas. See the
[CR-0005 test-batch/live-run record](../updates/2026-09-23-cr0005-test-batch-run-guide.md).
The private operator config was later supplied explicitly; this ADR contains
no coordinates/timezone values, and weather remains opt-in.

## Supersession

None. At decision time this ADR did not itself accept or waive any
S04/S06/S07/S10 implementation gate; subsequent CR-0005 add-on acceptance is
recorded separately in the [change record](../changes/CR-0005-configured-location-weather-solar-lighting.md).
