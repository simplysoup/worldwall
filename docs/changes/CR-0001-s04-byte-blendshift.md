# CR-0001 — Add exact byte-space BlendShift animation

- State: applied
- Proposer / date: Pi coding agent / 2026-09-22
- Decision owner: `simplysoup`
- Baseline version / affected IDs: approved v0.1; S04 / WW-04; ADR-0004
- Motivation / evidence: user visual comparison against the V29 reference at <https://jamez.it/8bit-wallpaper/>; the reference uses multiple Classic/BlendShift palette ranges, while the current implementation uses linear-RGB `palette_cycle` interpolation and produces a purple hue rotation. The reference source exposes V29 cycles for ranges 22–31, 32–47, 48–63, 64–79, 80–95, 96–103, and 128–143.

## Exact delta

| Item | Before | Proposed after | Reason |
| --- | --- | --- | --- |
| S04 animation semantics / ADR-0004 | `palette_cycle` interpolates between authored colors in linear RGB after sRGB decode | Add a distinct `blend_shift` effect that performs the reference algorithm in authored palette-byte space: rotate the ordered range by the integer amount, then interpolate each byte channel toward the preceding rotated entry for the fractional amount | Match the referenced V29 appearance without silently changing existing `palette_cycle` packages |
| Animation schema v1 | Effects are `static`, `hue_cycle`, `brightness_pulse`, and `palette_cycle` | Add `blend_shift` with ordered `range`/`indices`, signed bounded cycles-per-second, and the same immutable metadata packing | Preserve existing effects and make target compatibility explicit |
| GPU shader | Palette effects use linearized palette values | `blend_shift` reconstructs authored 8-bit palette values, performs byte-space interpolation, then normalizes for the sRGB presentation target; normal frames still update only the clock uniform | Reproduce Classic/BlendShift color behavior while retaining GPU work and no CPU pixel recoloring |
| V29 local fixture | Source-derived ranges are currently represented by the existing linear `palette_cycle` effect | Use `effect = "blend_shift"` for the recovered V29 ranges and normalize each source step rate to complete range cycles per second | Prevent the example from being presented as exact reference fidelity and avoid sub-frame animation loops |

## Alternatives and impact

Doing nothing preserves the accepted S04 implementation but does not match the
requested reference: hue rotation produces purple water and linear palette
interpolation has different direction/byte-rounding behavior.

The smallest correction is to add `blend_shift` beside the accepted
`palette_cycle` effect. Reinterpreting `palette_cycle` in byte space would be
smaller in code but would silently change existing schema semantics and make
previous S04 palette-cycle evidence ambiguous; that alternative is rejected.

The proposed effect reuses the existing per-index metadata and ordered cycle
index buffers, so it does not add a renderer resource or a per-frame upload.
The shader needs a byte-space palette helper and a new effect discriminator.
The CPU reference must define integer rotation, fractional interpolation,
rounding, and wrap behavior. Parser tests, byte-space CPU vectors, GPU
readback vectors, and a live V29 trace are required. Existing `static`,
`hue_cycle`, `brightness_pulse`, and `palette_cycle` behavior remains covered.

Compatibility impact is limited to packages that opt into the new effect.
The existing S04 ADR and acceptance evidence remain historical for the
unchanged effects; the S04 stage gate must gain coverage for the new effect
before claiming reference compatibility. Rollback is removal of the new
schema effect and fixture entry; existing packages continue to parse.

## Approval

- Decision / conditions: approved as an additive `blend_shift` effect; existing `palette_cycle` semantics remain unchanged
- Approver identity / date: `simplysoup` / 2026-09-22

## Application

- Applying revision / evidence: applied in the uncommitted working tree; implementation and live evidence in [the S04 BlendShift update](../updates/2026-09-22-s04-byte-blendshift.md)
- Updated contract version / files: delivery contract v0.2; ADR-0004; S04 stage/scene-format/README documentation; `src/animation.rs`; `src/renderer.rs`; tests; local V29 fixture metadata
- Accepted evidence made stale: none for existing effects; the new BlendShift gate is covered by CPU vectors, ignored GPU readback, and assigned-session live trace
- Linked roadblock disposition: none
