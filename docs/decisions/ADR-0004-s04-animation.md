# ADR-0004 — S04 palette animation schema and GPU packing

- Status: accepted; amended by [CR-0001](../changes/CR-0001-s04-byte-blendshift.md), [CR-0002](../changes/CR-0002-s06-time-of-day-palettes.md), and [CR-0005](../changes/CR-0005-configured-location-weather-solar-lighting.md) on 2026-09-22
- Owner / date: Pi coding agent / 2026-09-21
- Approval identity / date: `simplysoup` / 2026-09-21
- Affected stages / requirements: S04; WW-03, WW-04, WW-05
- Related change proposal: none; this is stage-local and does not change the approved baseline

## Context and constraints

S04 needs a complete, versioned authoring contract rather than the original
underspecified four-float animation texture. S03 supplies an opaque 256-entry
sRGB-authored palette. Normal frames must not recolor pixels or regenerate the
scene texture on the CPU. The GPU representation therefore needs immutable
per-palette-entry metadata plus a small monotonic clock uniform.

The implementation targets the pinned `wgpu` 30.0.1 / WGSL path and uses the
same indexed `R8Uint` scene and `Rgba8Unorm` palette resources accepted by
ADR-0003. The assigned maintainer approved this stage-local decision on
2026-09-21. That approval authorizes the schema/layout; it does not by itself
accept the S04 implementation gates.

## Decision

### Authoring schema v1

`animation.toml` is optional. An absent file means a static scene. A present
file must contain `schema_version = 1` and zero or more `[[group]]` tables.
Each group has:

```toml
name = "water"                    # unique, non-empty string
effect = "hue_cycle"              # static | hue_cycle | brightness_pulse | palette_cycle | blend_shift
range = [1, 3]                     # inclusive ordered range; or use indices below
# indices = [9, 4, 11]             # mutually exclusive ordered list
speed = 0.25                       # signed cycles per second
amplitude = 0.5                    # 0..=1; effect-specific magnitude
phase = 0.125                      # cycles, [0, 1)
```

`range` and `indices` are mutually exclusive. Ranges expand in ascending
order; lists preserve their written order. Indices are 8-bit palette labels,
groups may not overlap, and palette-cycle groups require at least two
members. Speeds are finite and bounded to +/-16 cycles/second. Amplitudes are
finite in 0..=1. Phases are finite in [0, 1). Unknown keys, effects, versions,
empty groups, duplicate names/indices, and non-finite or out-of-range values
are rejected before GPU allocation. Static groups require zero speed and
amplitude.

Effects operate on linear RGB values after the explicit S03 sRGB decode:

- `static`: returns the authored palette color.
- `hue_cycle`: converts linear RGB to HSV, offsets hue by
  `amplitude * fract(phase + time_seconds * speed)`, and converts back.
- `brightness_pulse`: multiplies linear RGB by
  `clamp(1 + amplitude * sin(2*pi*fract(phase + time_seconds * speed)), 0, 2)`.
- `palette_cycle`: for each ordered member, linearly interpolates in linear
  RGB between the current and next member, wrapping at the end. The
  interpolation amount is `fract(phase + time_seconds * speed)`.
- `blend_shift`: matches the reference Classic BlendShift operation in
  authored palette-byte space. The ordered range is rotated toward the
  preceding member by `floor(amount)`, where `amount` is
  `count * fract(phase + time_seconds * speed)`; `speed` remains complete
  range cycles per second. Each byte channel is then faded toward that
  preceding rotated value using
  `floor(fract(amount) * 100) / 100`, with the resulting channel itself
  floored like the reference implementation. The resulting authored sRGB
  bytes are decoded for the sRGB presentation target.

The animation shader always receives `time_seconds`, a monotonic phase bounded
by a 1024-second wrap. Scenes may additionally opt into a contained
`time_of_day.toml` resource whose immutable 256×N RGBA8 atlas and ordered civil-
second keyframes are supplied through a second bounded clock field. The shader
receives no weather, location, or selection state; civil time only selects/fades
rows of the already resident optional atlas.

### GPU layout

Each of the 256 palette entries receives one immutable 32-byte storage-buffer
record:

```text
effect:u32, group_offset:u32, member_count:u32, ordinal:u32,
speed:f32, amplitude:f32, phase:f32, reserved:f32
```

`effect` values are 0 static, 1 hue, 2 pulse, 3 palette cycle, and 4 byte-space
BlendShift. A second immutable 256-element `u32` storage buffer stores the
concatenated ordered cycle lists. An optional immutable RGBA8 256×N atlas and
16-byte-per-keyframe storage buffer holds time-of-day keyframes. The v1 16-byte
uniform stores monotonic `time_seconds`, civil seconds, keyframe count, and an
enabled flag. CR-0005 extends this to a 32-byte uniform carrying the
CPU-resolved solar row pair, interpolation fraction, and sample mode. Scene
indices, base palette, metadata, cycle lists, atlas, and keyframes are uploaded
once per scene. Each rendered frame writes only that bounded uniform.

This layout is representable on the pinned device path and leaves ample room
under ordinary storage/uniform limits. Runtime code still treats device
validation failure as a startup/recovery error rather than silently dropping an
effect.

## Alternatives

1. Four floats per palette entry: rejected because a palette-cycle ordered
   list cannot be represented without an additional resource.
2. CPU palette recoloring: rejected because it violates WW-03 and the S04
   steady-state upload invariant.
3. Store authored sRGB and interpolate it directly for the existing
   `palette_cycle`: rejected because the S03 color path requires explicit
   linear-RGB effect operations. CR-0001 adds the separate `blend_shift`
   effect only for compatibility with the reference's byte-space algorithm.
4. Civil-clock or weather-driven scene selection in WGSL: rejected. CR-0002
   permits only an explicit civil-seconds value to address an immutable optional
   palette atlas; weather, location, and selection remain outside the shader.

## Consequences and validation

The parser and CPU reference vectors are pure and dependency-free. The GPU
metadata and optional time-of-day resources are immutable; normal animation
updates write only a bounded frame uniform. Civil timeline tests cover
ordered keyframes, day wrap, authored-byte floor fading, atlas containment, and
optional-resource fallback. Scheduler tests use integer fake-clock nanoseconds and cover bootstrap,
15/30 FPS intervals, early callbacks, withheld callbacks, delayed callbacks,
resize bootstrap, and no catch-up bursts. The implementation evidence records
animated GPU readback, sustained bounded pacing, lifecycle traces, and an
observational resource baseline; no release limit or power claim is made by
this decision.

This decision is accepted, but S04 still requires its implementation,
animated GPU comparison, live failure-path, pacing, and resource-baseline gates.
If later evidence exposes a packing defect, retain static mode and append a
superseding decision rather than silently dropping palette-cycle behavior.

## Reference scene sources

At the user's request, the public Living Worlds catalog and three example
scene scripts were fetched for inspection on 2026-09-21:

- Catalog: <https://www.effectgames.com/demos/worlds/scenes.js>
- V26 winter forest: <https://www.effectgames.com/demos/worlds/cache/V26-01January-janclrscpt.js>
- V19 mountain stream: <https://www.effectgames.com/demos/worlds/cache/V19-02February-febclrscpt.js>
- V29 seascape: <https://www.effectgames.com/demos/worlds/cache/V29-09September-SEPTCLRCUMSCPT.js>

The example scripts embed 640×480 LBM palette/pixel data rather than linking
simple standalone PNG files. The fetched examples are provenance/reference fixtures only. Derived PNGs
and raw responses are deliberately kept in the local untracked
`fixtures/untracked/effectgames/` folders and are not part of the tracked or
published delivery. No license or redistribution permission is inferred. The
inspection metadata is retained outside Git at
`$HOME/.local/state/worldwall/s04-implementation-20260921/effectgames-example-sources.json`.

## Approval record

`simplysoup` approved ADR-0004 on 2026-09-21. `simplysoup` approved CR-0001
on 2026-09-22, adding `blend_shift` and the byte-space reference semantics
recorded above without changing existing effect behavior. `simplysoup` approved
CR-0002 on 2026-09-22. It adds the optional civil-time atlas/keyframe resource
and bounded civil-seconds uniform without changing the base palette or selector
contract. `simplysoup` approved CR-0005 on 2026-09-22; [ADR-0008](ADR-0008-cr0005-configured-location-solar-palette.md)
records the solar-v2 anchor model and 32-byte resolved-sample uniform. This approval covers
the version-1 authoring fields, effect semantics, metadata packing, bounded
animation clock, and validation ranges recorded above. Stage acceptance is
recorded separately in the S04 implementation and acceptance updates after
review of S04-AC1 through S04-AC5.

## Supersession

None. CR-0001, CR-0002, and CR-0005 are approved additive amendments; the original
linear-RGB `palette_cycle` behavior and packages without a timeline remain
unchanged.
