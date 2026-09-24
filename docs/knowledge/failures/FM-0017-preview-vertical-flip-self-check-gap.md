# FM-0017 — WebGL preview flipped vertically while a single-row palette self-check stayed green

**Classification:** resolved-with-regression-test (2026-09-23; previously
reproduced from a user demo report) · **Areas:** curator preview, S12-AC6 ·
**Related:** [AC3–AC7 implementation update](../../updates/2026-09-23-s12-ac3-ac7-editing-runtime-filter-preview.md),
[learning workflow](../../workflows/learning.md)

## Trigger

Running `worldwallctl curator` and selecting a scene whose appearance has a
clear up/down orientation (reported by the maintainer from the first live demo:
"a single scene rendered upside down").

## Symptom

The WebGL2 preview rendered the indexed scene mirrored vertically (sky at the
bottom), while the in-page fidelity self-check reported an exact match and the
scene preview otherwise showed correct pixels, palette, and animation inputs.

## Suspected mechanism

The WGSL reference shader computes `output_position.y = uOutput.y -
position.y` because wgpu fragment `position` is top-left origin while the
intermediate cover mapping works in bottom-up coordinates, and the shader
flips source Y once more at the texture lookup. The WebGL port substituted the
WGSL `position.y` with `uOutput.y - gl_FragCoord.y`, but `gl_FragCoord.y` is
already the bottom-up coordinate (`uOutput.y - position.y` numerically), so the
extra inversion flipped the presented image.

## Confirmed cause

Confirmed by code derivation against the WGSL shader and by the fix: the port
now passes `gl_FragCoord.xy` unchanged into the shared mapping, which
reproduces the WGSL row mapping exactly (presented top row ↔ source top row).

## Contributing guardrail gap (the reason this reached a demo)

The fidelity self-check rendered a **1-row** (256×1) index strip and compared
palette bytes per index. A single-row target is invariant under a vertical
flip, so the check could not detect spatial orientation errors even though it
covered the full palette.

## Impact

Preview presentation only: the runtime wallpaper path (WGSL) was correct, and
palette/animation fidelity was unaffected. Any visual conclusion about scene
orientation from the curator preview before the fix was invalid.

## Prevention / recovery

- The preview self-check now renders a 32×8 index grid whose indices are a
  bijection of all 256 palette entries (full palette coverage preserved) and
  whose layout is asymmetric under vertical/horizontal flips. It verifies the
  shader readback per texel — palette **and** spatial mapping — against the
  offline-tested reference twin (`src/bin/curator_palette.js`).
- Live verification on the assigned host: "Preview self-check: shader palette
  and spatial mapping match the reference twin exactly (768 texels × 3 probe
  times)" with a correctly oriented Monolith Plains preview.

## Regression test

The in-page spatial self-check runs on every scene selection and ToD change and
must report an exact match; it is the recorded regression guard for this
mechanism. Related offline chain unchanged (Rust goldens ↔ JS twin ↔ shader).

## Revalidation trigger

Any change to the preview shader's coordinate mapping, the strip geometry, or
the readback self-check.
