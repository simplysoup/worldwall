# 2026-09-21 — S03 indexed-scene implementation increment

**Type:** bounded implementation increment and gate acceptance; S03 is accepted.
**Operator:** Pi coding agent.
**Reviewer / acceptance authority:** `simplysoup`.
**Decision:** ADR-0003 and S03/M1 accepted by `simplysoup` on 2026-09-21.
**Related requirements:** WW-02, WW-03, WW-11; S03-AC1 through S03-AC5 evidence submission.

## Bounded outcome

Add a validated static indexed-scene path without changing the accepted S02
Wayland ownership boundary: decode bounded 8-bit grayscale/indexed PNGs into
raw labels, validate an opaque 256 × 1 RGBA8 palette, upload `R8Uint` indices
and palette resources once, and render them through integer `textureLoad` with
centered nearest-neighbor cover mapping. The old solid-color path remains
available through `WORLDWALL_SOLID_COLOR=1`.

The exact decoder/color/size policy is fixed by accepted [ADR-0003](../decisions/ADR-0003-s03-scene-decoding-color.md).

## Changes

- Added `src/scene.rs` with PNG decoding, 16 MiB encoded and 16,777,216-pixel
  limits, raw-index preservation, opaque palette validation, a package-pair
  loader for `scene.png`/`palette.png`, a synthetic reference scene, and pure
  cover-coordinate math.
- Added `png = 0.17.16` to the pinned dependency set and lockfile.
- Added renderer resources for `R8Uint` indices, an authoritative `Rgba8Unorm`
  palette, a 16-byte output mapping uniform, integer `textureLoad`, explicit
  sRGB-to-linear conversion, and an sRGB-only presentation-format policy.
- Kept scene textures and palette resident across resize; resize only updates
  the output mapping uniform. Startup emits an upload generation/count trace.
- Added `WORLDWALL_SCENE_DIR` for a bounded fixed package path. `scene.toml`,
  package traversal/symlink policy, registry, and selection remain S05 work.
- Added in-memory synthetic PNG fixtures covering grayscale/indexed 0/255
  preservation, transparency rejection, opaque palette validation, malformed
  shape rejection, encoded-size rejection, and cover vectors.

## Verification

| Check | Exact command / procedure | Observed result | Outcome |
| --- | --- | --- | --- |
| Rust formatting | `cargo fmt --all` | Completed without changes required after the implementation pass | pass |
| Pure/unit tests | `cargo test --locked --all-targets --all-features` | 17 non-ignored tests passed, including 8 S03 scene tests and the prior S01/S02 tests | pass |
| Lint | `cargo clippy --locked --all-targets --all-features -- -D warnings` | Completed with no warnings | pass |
| Locked live bootstrap | `timeout --signal=TERM --kill-after=3s 20s cargo run --locked` | Assigned Wayland/Vulkan session configured `1920x1080`, selected `Rgba8UnormSrgb`, uploaded 6 index bytes and 1024 palette bytes once, requested callback before presentation, and presented one indexed frame; timeout 124 was intentional | pass for bootstrap observation; not stage acceptance |
| GPU reference readback | `cargo test --locked renderer::tests::gpu_indexed_reference_readback_matches_palette_and_cover_vectors -- --ignored --exact --nocapture` | Vulkan 4×2 readback matched CPU palette/cover vectors | pass |
| Live resize/input/teardown | Assigned-host mode restore, virtual input, and `tools/run_nested_s01.sh` | Textured path resized/restored without scene reupload, ordinary app retained input, and both nested teardown paths passed | pass |
| Documentation/tool checks | `python3 tools/check_docs.py`; `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/tests -v` | 66 Markdown files accepted; 33 tooling tests passed | pass |

| Locked build | `cargo build --locked` | Completed successfully | pass |

The live bootstrap log was retained at `/tmp/worldwall-s03-live.log` during the
bounded run. It is supplemental runtime evidence, not a color readback,
reference comparison, resize, input, or teardown proof.

## Open gates and limitations

- ADR-0003 and all S03 criteria are accepted. GPU readback/reference vectors and
  assigned-host textured resize/input/teardown evidence are recorded in the
  linked S03 evidence record.
- PNG package loading intentionally does not parse `scene.toml` or enforce
  package containment; those boundaries belong to S05. The fixed `scene.png` /
  `palette.png` path is not a registry or selection implementation.
- No art assets were added. The default scene is synthetic and original.

## Next safe action

Keep S03 as the M1 baseline. Before beginning S04, close its animation
schema/GPU-packing decision; do not start weather, registry, or selection work.
