# Scene format — proposed version 1

S03's exact decoder limits and color path are fixed by accepted [ADR-0003](decisions/ADR-0003-s03-scene-decoding-color.md). The broader authoring contract remains proposed where later stages own metadata and animation.

**State:** proposed authoring contract; approve through [ADR-0001](decisions/ADR-0001-delivery-baseline.md), then freeze exact decoding/color rules before S03 and animation packing before S04. No loader or assets exist yet.

## Scene package

```text
<scene-id>/
├── scene.toml
├── scene.png
├── palette.png
└── animation.toml       # optional; absent means static
```

```toml
schema_version = 1
id = "village"
image = "scene.png"
palette = "palette.png"
animation = "animation.toml"
times = ["day", "dusk"]
weather = ["clear", "cloudy"]
seasons = ["spring", "summer"]
```

Example paths are proposed, not supplied files. S05 adds validated tags around the S03 decoding contract. Relative paths must remain within the scene package after canonical resolution; reject traversal and escaping symlinks. Reject duplicate IDs, unsupported versions/tags, missing required files, and excessive dimensions before allocating GPU memory. Enforce documented encoded-size and decoded-size limits established in S03.

Empty/missing tag lists mean wildcard compatibility. The enumerations are `dawn/day/dusk/night`, `clear/cloudy/rain/snow/storm/fog/unknown`, and `spring/summer/autumn/winter/unknown`. Unknown environment values do not pretend to match a specific tag. Registry-level fallback is an explicitly designated, valid scene that works offline; invalid or empty registries produce a clear error, never an unchecked index/panic.

## Image and palette

- `scene.png` must be 8-bit grayscale used as raw index bytes, or 8-bit indexed-color PNG whose raw indices are preserved. Reject other bit depths/color types in v1 rather than silently quantizing. The current S03 proposal caps encoded input at 16 MiB and decoded image size at 16,777,216 pixels.
- The separate palette is authoritative. An embedded indexed-PNG palette is not a second live palette. Reject transparency metadata for index images; indices are labels, not grayscale colors or alpha.
- `palette.png` must decode losslessly to 256 × 1 opaque RGBA8 entries. Validate alpha is 255. v1 wallpapers are opaque; transparent entries are rejected.
- Upload indices as `R8Uint`; use integer `textureLoad`, not filtering or normalized reconstruction. Values 0 and 255 are explicit test fixtures.
- Palette byte values are sRGB-authored. ADR-0003 fixes the path: upload them as `Rgba8Unorm`, convert sRGB to linear explicitly in WGSL, and require an sRGB presentation target for output encoding. Capture color reference/readback evidence for S03.
- Cover scaling preserves aspect ratio, centers the crop, and calculates clamped integer source coordinates. Test odd sizes, both aspect directions, downscaling, edges, and scale changes. Pixel replication need not be integral at arbitrary output sizes; interpolation is still forbidden.

## Animation authoring versus GPU encoding

The original `animation.png → RGBA16Float` mapping was unspecified. Proposed authoring uses finite numeric TOML fields, converted once on scene load. The GPU representation is not a file-format promise.

Required effects: static, hue cycle, brightness pulse, and palette-cycle interpolation. A group assigns a shared effect, speed, amplitude, and phase to a specified palette range or index list. Palette cycling additionally needs an explicit ordered range/list and wrap/interpolation semantics. Reject ambiguous overlapping groups, out-of-range indices, non-finite numbers, and values that cannot be represented by the selected GPU encoding.

Before S04, an ADR must define:

1. Exact TOML field names/defaults/units and validation ranges, including cycles-per-second versus radians.
2. Pulse amplitude/clamping and the color space used for hue/brightness/interpolation.
3. Group membership, cycle endpoints/direction, signed speed support, and phase wrapping.
4. Shader layout, packing/precision, effect discriminants, and all required resources.
5. CPU reference vectors and GPU comparison tolerances for every effect.

`RGBA16Float`, 256 × 1, is a candidate for simple per-index parameters, not proof that four channels encode all effects. Additional read-only metadata/palette resources are allowed if justified by the approved layout decision and resource budget. CPU pixel animation is not an alternative. Freeze authoring v1 before shipping a converter; reject unsupported layouts explicitly.

## Selection policy

S05 implements a pure score with explicit Boolean matches: time × 5, weather × 4, season × 2, plus an injected random value in `[0, 1)`. Wildcards match known and unknown values; otherwise unknown values match only explicit `unknown` tags where allowed. Stable ID ordering resolves exact ties. Seed and environment fixtures make tests repeatable.

A scene is compatible for `next` when each tag category matches (wildcards included). Exclude the current ID when another compatible candidate exists; otherwise report a no-op. Normal scored selection can choose a partial match; when no scene has any environmental match, use the designated fallback. Manual ID selection bypasses tag scoring but not asset validation. Record the selected ID, trigger, and non-sensitive rationale for diagnosis.

Do not persist a borrowed selector return into mutable registry operations. Return a stable ID/handle or otherwise define ownership/lifetimes in real code. A failed load or upload must not discard the previous scene.
