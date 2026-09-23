# Scene format — package v1/v2, palette timeline v1/v2

S03's exact decoder limits and color path are fixed by accepted [ADR-0003](decisions/ADR-0003-s03-scene-decoding-color.md). S04 animation, S05 registry/selection, and CR-0002 civil timelines are accepted. CR-0005 adds the opt-in solar timeline v2; its implementation gates remain separate from the historical stage acceptances.

**State:** package v1 decoding/color, S04 animation, and S05 registry contracts are implemented. Scene-package schema v2 adds the CR-0006 family/band metadata; it is required only for an explicitly enabled adaptive registry. CR-0007 adds a schema-v4 global animation-speed multiplier in runtime config; scene animation manifests continue to store authored speeds. `time_of_day.toml` schema v1 is civil-second based; schema v2 is an opt-in solar-event timeline under [CR-0005](changes/CR-0005-configured-location-weather-solar-lighting.md) and [ADR-0008](decisions/ADR-0008-cr0005-configured-location-solar-palette.md).

## Scene package

```text
<scene-id>/
├── scene.toml
├── scene.png
├── palette.png
├── animation.toml       # optional; absent means static
└── time_of_day.toml     # optional; contained civil-time v1 or solar v2 palette timeline
```

```toml
schema_version = 1
id = "village"
image = "scene.png"
palette = "palette.png"
animation = "animation.toml"
time_of_day = "time_of_day.toml"
times = ["day", "dusk"]
weather = ["clear", "cloudy"]
seasons = ["spring", "summer"]
```

Example paths are proposed, not supplied files. S05 adds validated tags around the S03 decoding contract. Relative paths must remain within the scene package after canonical resolution; reject traversal and escaping symlinks. Reject duplicate IDs, unsupported versions/tags, missing required files, and excessive dimensions before allocating GPU memory. Enforce documented encoded-size and decoded-size limits established in S03.

CR-0003 records the complete Effect Games reference inventory in
[`fixtures/effectgames-reference-manifest.toml`](../fixtures/effectgames-reference-manifest.toml).
It is metadata/provenance only: stable family and variant IDs are not renderer
selection triggers, and catalog names/months/scripts/weather labels do not
become automatic selection inputs. The 21 available catalog-listed variants
can be derived into local ignored packages with
`python3 tools/generate_effectgames_fixtures.py`; the December V12 entry is
explicitly `source-unavailable` and is not synthesized. Source-specific
palette remaps are applied by that derivation tool, not by hidden renderer
behavior. See the [CR-0003 evidence](evidence/2026-09-22-cr003-scene-variants.md)
for parser/registry results and licensing limitations.

Empty/missing tag lists mean wildcard compatibility. The enumerations are `dawn/day/dusk/night`, `clear/cloudy/rain/snow/storm/fog/unknown`, and `spring/summer/autumn/winter/unknown`. Unknown environment values do not pretend to match a specific tag. Registry-level fallback is an explicitly designated, valid scene that works offline; invalid or empty registries produce a clear error, never an unchecked index/panic.

## Registry root (S05 implementation increment)

A registry root contains an explicit `registry.toml` and one directory per
listed scene. The explicit list avoids accidental package discovery:

```toml
schema_version = 1
fallback = "village"
scenes = ["village", "stream"]
```

Each listed directory contains `scene.toml` and the package files described
below. The S05 loader requires a non-empty list and a fallback whose manifest
and assets validate completely. It canonicalizes the registry, package, and
asset paths, rejects absolute/parent paths and symlink escapes, and bounds
manifest/animation reads before decoding. Invalid registry state is rejected
without replacing a previously active scene. CR-0006 adds an opt-in stricter
registry contract; it does not change ordinary schema-v1/v2 selection behavior.

## Adaptive scene-family metadata (CR-0006)

Scene-manifest schema v2 may add explicit `family` and `temperature_band`
fields. The TOML key is `family`; `family_id` is the internal model name, not an
accepted manifest key. They are metadata, not inferred from a package ID or catalog caption.
Ordinary registries may omit them; a registry used with
`[weather].adaptive_scenes = true` must validate every package as an adaptive
variant, provide a clear variant for every family, cover all five approved
bands, and designate clear `v29-seascape` as its offline fallback. Each
adaptive package has exactly one `weather` tag from `clear`, `cloudy`, `rain`,
or `snow`.

Example manifest metadata:

```toml
schema_version = 2
id = "v29-seascape"
image = "scene.png"
palette = "palette.png"
weather = ["clear"]
family = "v29-seascape"
temperature_band = "warm"
```

The approved family mapping is cold (`T ≤ 0°C`): V26 Winter Forest; cool
(`0 < T ≤ 10°C`): V19 Mountain Stream / V16 Mirror Pond; mild
(`10 < T ≤ 20°C`): V30 Deep Forest / VW3B Monolith Plains / V20 Crystal
Caves; warm (`20 < T ≤ 28°C`): V08 Jungle Waterfall / V29 Seascape / CORAL
Aquarius; hot (`T > 28°C`): V25 Desert. Keep the prior band until the
adjacent boundary is crossed by more than 2°C. First start without a valid
prior state or usable weather uses V29; later full-selection triggers use the
persisted family/band and current available temperature. The private atomic
state stores only family ID and band—never location or weather values.

A fresh known weather appearance may request an exact precipitation variant
only within the active family. Same-family fallbacks are snow → cloudy → clear,
rain → cloudy → clear, and cloudy → clear. Unknown, stale, failed, or missing
candidates retain the active package. Weather cannot change family; only
startup/resume/explicit full-selection policy can. `scene <id>` and `next`
remain explicit existing selection commands. Package availability is not
implied by the catalog: Winter Manor is excluded as non-animated, V12 is
source-unavailable, and V05 remains excluded pending time-window behavior.
Monolith Plains has a private review crop derived from the supplied red guide
using half-open rectangle `(18, 30, 346, 476)`; the source fixture and guide
remain unchanged. Animation speed is applied globally by CR-0007, so the V29
clear/cloudy siblings retain source-authored speeds rather than receiving
separate per-package edits. These clarifications do not by themselves accept
adaptive rollout gates. See [CR-0006](changes/CR-0006-weather-adaptive-scene-selection.md)
and [CR-0007](changes/CR-0007-global-animation-speed-multiplier.md).

## Image and palette

- `scene.png` must be 8-bit grayscale used as raw index bytes, or 8-bit indexed-color PNG whose raw indices are preserved. Reject other bit depths/color types in v1 rather than silently quantizing. The current S03 proposal caps encoded input at 16 MiB and decoded image size at 16,777,216 pixels.
- The separate palette is authoritative. An embedded indexed-PNG palette is not a second live palette. Reject transparency metadata for index images; indices are labels, not grayscale colors or alpha.
- `palette.png` must decode losslessly to 256 × 1 opaque RGBA8 entries. Validate alpha is 255. v1 wallpapers are opaque; transparent entries are rejected.
- `time_of_day.toml` and its optional `time_of_day.png` atlas are contained immutable resources. The atlas is 256 × N opaque RGBA8 rows (N 1..=256); each timeline has 2..=256 keyframes with an in-range `row`. Schema v1 requires strictly ordered `civil_seconds` in `[0, 86_400)` and preserves its day-wrap byte-floor fade. Schema v2 requires `basis = "solar"`; each keyframe names `event = "sunrise" | "sunset"` and signed `offset_minutes` in [-720, 720]. Repeated rows are allowed. Omission preserves the existing one-palette behavior.
- Upload indices as `R8Uint`; use integer `textureLoad`, not filtering or normalized reconstruction. Values 0 and 255 are explicit test fixtures.
- Palette byte values are sRGB-authored. ADR-0003 fixes the path: upload them as `Rgba8Unorm`, convert sRGB to linear explicitly in WGSL, and require an sRGB presentation target for output encoding. Capture color reference/readback evidence for S03.
- Cover scaling preserves aspect ratio, centers the crop, and calculates clamped integer source coordinates. Test odd sizes, both aspect directions, downscaling, edges, and scale changes. Pixel replication need not be integral at arbitrary output sizes; interpolation is still forbidden.

## Time-of-day palette authoring and GPU encoding

A timeline is optional and does not select a scene. Its example shape is:

```toml
schema_version = 1
atlas = "time_of_day.png"

[[keyframe]]
civil_seconds = 22500
row = 0

[[keyframe]]
civil_seconds = 23400
row = 1
```

For schema v1, the environment owner supplies explicit local civil seconds to
the resident renderer. The GPU locates surrounding rows, wraps at 86,400
seconds, fades RGBA byte channels with the reference floor rule, then decodes
the result through the existing color path. Clock ticks and weather completion
do not invoke selection or upload the atlas again.

Schema v2 opts in with solar anchors:

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

The configured location and IANA timezone determine the local calendar date.
The environment layer computes solar events for previous/current/next local
dates and resolves a bounded lower-row/upper-row/fraction sample. WGSL samples
the immutable atlas and applies the authored-byte floor fade before ordinary
palette effects. Missing location/timezone, polar or unavailable events,
duplicate resolved anchors, or an unbracketed time selects the base palette for
the solar timeline; it does not invent events. The atlas remains one-time scene
resource data. Clock/solar changes never select a scene. Weather changes only the resident same-family precipitation variant when the CR-0006 opt-in is valid; family selection remains limited to its full-selection triggers.

## Animation authoring versus GPU encoding

The original `animation.png → RGBA16Float` mapping was unspecified. Proposed authoring uses finite numeric TOML fields, converted once on scene load. The GPU representation is not a file-format promise.

Required effects: static, hue cycle, brightness pulse, linear-RGB palette-cycle interpolation, and authored-byte BlendShift. A group assigns a shared effect, speed, amplitude, and phase to a specified palette range or index list. Palette cycling and BlendShift additionally need an explicit ordered range/list and wrap/interpolation semantics. BlendShift uses the reference integer rotation plus 1/100 fractional byte-channel fade before the result is decoded for the sRGB presentation target. Reject ambiguous overlapping groups, out-of-range indices, non-finite numbers, and values that cannot be represented by the selected GPU encoding.

Before S04, an ADR must define:

1. Exact TOML field names/defaults/units and validation ranges, including cycles-per-second versus radians.
2. Pulse amplitude/clamping and the color space used for hue/brightness/interpolation.
3. Group membership, cycle endpoints/direction, signed speed support, and phase wrapping.
4. Shader layout, packing/precision, effect discriminants, and all required resources.
5. CPU reference vectors and GPU comparison tolerances for every effect.

`RGBA16Float`, 256 × 1, is a candidate for simple per-index parameters, not proof that four channels encode all effects. Additional read-only metadata/palette resources are allowed if justified by the approved layout decision and resource budget. CPU pixel animation is not an alternative. Freeze authoring v1 before shipping a converter; reject unsupported layouts explicitly.

### Global animation-speed multiplier (CR-0007)

Runtime config schema v4 accepts top-level `animation_speed_multiplier` in
`0.0..=1.0`; the default is `0.5`, zero pauses animated palette effects, and
one uses authored speed. Missing values in schema v1-v3 and environment-backed
configuration also default to `0.5`. The multiplier affects every dynamic
palette effect, including BlendShift, hue cycle, brightness pulse, and palette
cycle; it does not alter static indices, civil-time/solar atlas sampling, or
scene selection. The monotonic animation clock rate-integrates elapsed time,
so a successful `reload-config` changes speed without rewinding phase or
selecting a scene. Keep authored `animation.toml` speed values unchanged.

## Selection policy

S05 implements a pure score with explicit Boolean matches: time × 5, weather × 4, season × 2, plus an injected random value in `[0, 1)`. Wildcards match known and unknown values; otherwise unknown values match only explicit `unknown` tags where allowed. Stable ID ordering resolves exact ties. Seed and environment fixtures make tests repeatable.

A scene is compatible for `next` when each tag category matches (wildcards included). Exclude the current ID when another compatible candidate exists; otherwise report a no-op. Normal scored selection can choose a partial match; when no scene has any environmental match, use the designated fallback. Manual ID selection bypasses tag scoring but not asset validation. Record the selected ID, trigger, and non-sensitive rationale for diagnosis.

Do not persist a borrowed selector return into mutable registry operations. Return a stable ID/handle or otherwise define ownership/lifetimes in real code. A failed load or upload must not discard the previous scene.

## Pool eligibility (CR-0008 / S12 target; not implemented)

Approved contract v0.8 adds an optional schema-v5 `scene_pools_file` reference to a separately versioned pool/availability sidecar. The curator's local library and package sources remain separate from the CR-0006 adaptive registry; the selector may draw candidates from the configured source pools without weakening adaptive-registry validation. With no sidecar, existing registry selection behavior is unchanged.

When configured, every automatic selection path—including startup/full selection, resume, explicit `reload`, `next`, and an approved CR-0006 same-family weather-variant replacement—must select only a scene in an enabled pool that is itself enabled and whose configured time, temperature, and weather criteria all match the same environment snapshot. Multiple accepted weather values within one rule are alternatives; enabled criteria categories combine with AND. A required but unknown environment value does not match. `next` preserves stable ordering and wrap behavior. No clock/weather/config-reload trigger is added. `reload-config` applies sidecar changes transactionally without selecting a scene.

Manual `scene <id>` is an explicit override of pool and availability rules, while still requiring the selected package to be known, valid, and loadable. An empty eligible set must never cause selection of a hidden/ineligible fallback: startup reports no eligible scene, `next` returns a no-op reason, and other attempts retain the resident scene. The exact sidecar schema, candidate-source composition, adaptive-family/hysteresis interaction, and startup selection-unavailable diagnostic/exit representation are S12 acceptance work. This target does not make the planned `worldwallctl curator` or schema v5 available in the current binary.
