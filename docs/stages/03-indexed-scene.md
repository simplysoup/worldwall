# S03 — Static indexed scene (M1)

## Purpose

Deliver the first real wallpaper MVP: a static indexed image, correctly colored/scaled, with safe resize and teardown. Requirements: WW-02, WW-03, WW-11. Dependency: S02. State/owner: [roadmap](../roadmap.md).

## Entry conditions

S02 accepted. Approve the decoding/color-path/size-limit decision from [scene format](../scene-format.md) before implementing the loader. Lawful reference fixtures and expected pixel values must be available.

## Deliverables

Validated PNG decoding, `R8Uint` upload, authoritative 256 × 1 palette, integer shader lookup, nearest-neighbor centered cover mapping, and reference tests. A fixed test scene is sufficient; registry/selection is S05.

## Execution

1. Generate fixtures covering indices 0/255, repeated colors, sharp edges, odd dimensions, both accepted PNG encodings, malformed sizes/types, and non-opaque palette entries.
2. Decode raw index bytes without palette expansion, quantization, or normalization. Check encoded/decoded limits and texture/device limits before allocation.
3. Choose a color-correct palette/output path and document where sRGB decoding/encoding occurs. Validate upload layout/alignment against actual wgpu APIs.
4. Bind integer indices and palette; use `textureLoad` with explicit integer coordinates. Draw a fullscreen triangle without a vertex buffer.
5. Implement centered cover mapping with clamped coordinates; unit-test portrait/landscape/wide output, downscale, odd pixel edges, and resize.
6. Compare GPU output/readback against reference vectors under the documented color tolerance. Use live captures for appearance as supplemental evidence.
7. Count uploads and show that an unchanged static scene does not regenerate/upload image data. Exercise S02 lifecycle with the textured path.

## Acceptance criteria

- [ ] **S03-AC1:** Both supported PNG index paths preserve all 256 possible values; unsupported/corrupt/oversized input is rejected predictably before unsafe allocation.
- [ ] **S03-AC2:** GPU/reference comparisons prove index/palette correctness and color handling; no normalized index sampling, unwanted filtering, or transparency ambiguity.
- [ ] **S03-AC3:** Cover scaling/crop and source bounds pass numeric fixtures and live resize/scale checks without smearing or out-of-bounds access.
- [ ] **S03-AC4:** Live textured background remains input-free, resizes, and exits cleanly; scene uploads occur only at load/recovery, not steady-state frames.
- [ ] **S03-AC5:** Reviewer accepts M1 after S01–S03 evidence; setup/run instructions describe the actual working demo.

## Failure and rollback

If decoding or upload fails at startup, show an actionable diagnostic/controlled exit; once a valid scene exists, retain it when candidate assets fail. Keep the S02 solid-color diagnostic available in tests. Never silently convert unsupported RGB art to indices or “fix” color by unrecorded gamma guesses.

## Handoff

Publish fixture provenance, decoder/schema constraints, color-path ADR, GPU comparison tolerances, scaling tests, and the M1 demo procedure. No weather, registry scoring, or animation is needed to accept this stage.
