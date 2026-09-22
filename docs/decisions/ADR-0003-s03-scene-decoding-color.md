# ADR-0003 — S03 scene decoding, limits, and color path

- **Status:** accepted
- **Date proposed:** 2026-09-21
- **Decision owner / approver:** maintainer `simplysoup`
- **Acceptance date:** 2026-09-21
- **Affected stages:** S03
- **Evidence:** [S03 implementation increment](../updates/2026-09-21-s03-indexed-scene.md)

## Context

S03 needs a bounded PNG decoder that preserves index labels, an authoritative
palette, and a documented color path. Generic image decoding may expand indexed
PNG data to RGB, and an unchecked PNG header or compressed stream can cause an
unbounded allocation. The accepted S02 renderer currently presents a solid
fullscreen triangle; S03 must add resources without changing its Wayland
ownership boundary.

## Decision

1. Accept only non-animated PNGs whose scene image is 8-bit grayscale or
   8-bit indexed color. Decode into one raw byte per source pixel. Reject other
   bit depths, color types, animation, transparency metadata, zero dimensions,
   and dimensions above 16,777,216 pixels.
2. Accept only a 256 × 1, 8-bit RGBA palette PNG. It is authoritative and every
   alpha byte must be 255. Embedded indexed-PNG palette bytes are not used as a
   live palette.
3. Reject an encoded PNG larger than 16 MiB before invoking the decoder. Give
   the pinned `png` decoder a 64 MiB internal allocation limit, then validate
   dimensions and decoded output size before allocating the output byte vector.
4. Upload source indices as `R8Uint` and use WGSL `textureLoad` with integer
   coordinates. Upload palette bytes as `Rgba8Unorm`; explicitly convert the
   authored sRGB bytes to linear RGB in the fragment shader. Require an sRGB
   presentation format so wgpu performs the corresponding linear-to-sRGB output
   encoding. No filtering or normalized index sampling is permitted.
5. Map output pixels with centered nearest-neighbor cover scaling. The CPU
   reference function and shader use the same scale/crop/clamp contract. Resize
   updates only the output-size uniform; scene textures are not re-uploaded.

The current implementation follows this decision. This ADR acceptance does not
accept S03; the stage still requires its GPU/reference and live acceptance
gates.

## Alternatives

1. Expand all PNGs to RGB on the CPU: rejected because it loses exact labels and
   violates the indexed-resource requirement.
2. Use an sRGB palette texture and rely on implicit `textureLoad` behavior:
   deferred because the conversion semantics are less explicit for the pinned
   integer-load path.
3. Accept non-sRGB presentation formats and guess output gamma: rejected because
   it would make the color result device/format dependent without a tested
   conversion path.
4. Permit transparent palette entries: rejected for the opaque wallpaper MVP;
   alpha semantics belong to a future approved format change.

## Consequences

The decoder and resource path are small and testable, but an output advertising
no sRGB format is rejected rather than rendered with an undocumented gamma path.
The encoded and decoded limits are conservative application policy, not a claim
that all PNGs under those limits are safe or lawful artwork. S05 still owns
`scene.toml`, package traversal/symlink policy, registry metadata, and selection.

## Approval record

`simplysoup` accepted ADR-0003 on 2026-09-21, authorizing the exact limits,
explicit sRGB conversion path, and reference-vector policy recorded above.
S03 remains subject to its separate acceptance evidence and review.
