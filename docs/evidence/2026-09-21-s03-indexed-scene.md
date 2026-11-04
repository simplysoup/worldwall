# Evidence — S03 static indexed scene / M1

- **Date / operator:** 2026-09-21 / Pi coding agent
- **Revision or artifact hashes:** working tree; source hashes are recorded below
- **Stage / requirements:** S03; WW-02, WW-03, WW-11; S03-AC1 through S03-AC5
- **Decision:** ADR-0003 accepted by `simplysoup` on 2026-09-21
- **Environment:** Linux x86_64, kernel `7.2.4-zen2-1-zen`, Hyprland 0.56.2, Vulkan 1.4.357, NVIDIA GeForce RTX 3070 Ti, Rust/Cargo 1.94.0, wgpu 30.0.1, SCTK 0.21.1; assigned outputs 1920×1080 scale 1; nested output 931×941 scale 1
- **Inputs:** original synthetic 3×2 index scene with labels 0, 1, 2, and 255; 256×1 opaque RGBA8 palette; no network or bundled art assets
- **Kind:** pure/unit, ignored GPU integration/readback, assigned-session live compositor, nested real-Hyprland lifecycle

## Source identity

```text
c79b30669bbafb7ba4de0d619d982792282b868e0f9ccd7d5d8c48656ddc0ca6  Cargo.toml
23a45c24908be88ed7f32acb30cbe3bcac34e77eb180917fdc454d9714015d92  Cargo.lock
9061fe1592f426f357e5ab2cf391806883473d3c56869a957cd8d67010e66d27  src/scene.rs
15406b61edcd918428ac15d9e6173d2c7b232afbb340b7408960d0df8b745158  src/renderer.rs
28e558f332943f01ec98257acd4ab13bfd900bd05346a8b2402e91054fd90681  src/wayland_surface.rs
3e32a87fb0c9cef66951c7229379e4c26adb134c57eda54f340478cb8e816553  docs/decisions/ADR-0003-s03-scene-decoding-color.md
9a05793c090134af0367e2b6271f8f57ae8d3e39f52199bf0848d5a6436b185e  docs/updates/2026-09-21-s03-indexed-scene.md
```

The retained runtime artifacts and their checksums are in
`$HOME/.local/state/worldwall/s03-review-20260921-204011/artifacts.sha256`.
The synthetic PNG package used for the live package-loader run is outside the
repository; its `scene.png` and `palette.png` hashes are retained there.

## Results

| Criterion | Exact command or procedure | Expected | Observed | Outcome | Artifact |
| --- | --- | --- | --- | --- | --- |
| **S03-AC1** | `cargo test --locked --all-targets --all-features` | Both accepted PNG paths preserve all 256 byte values; corrupt, unsupported, transparent, and oversized inputs reject predictably before unsafe allocation | 17 non-ignored tests passed. The grayscale and indexed tests decode every value `0..=255`; wrong type, transparency, malformed palette shape, and encoded-size tests reject. The loader checks file metadata and uses a bounded read before invoking `png` | **pass** | `cargo-test.log`; `src/scene.rs`; `artifacts.sha256` |
| **S03-AC2** | `cargo test --locked renderer::tests::gpu_indexed_reference_readback_matches_palette_and_cover_vectors -- --ignored --exact --nocapture` | GPU output matches index/palette reference bytes with the documented explicit sRGB path and no filtering/normalized index sampling | Vulkan headless render/readback passed for a 4×2 target. The test uses `R8Uint`, integer `textureLoad`, an unfilterable palette binding, explicit sRGB-to-linear WGSL conversion, an sRGB target, and compares every RGBA8 readback pixel to the CPU palette/cover vector | **pass** | `gpu-readback.log`; `src/renderer.rs`; `src/scene.rs` |
| **S03-AC3** | `cargo test --locked --all-targets --all-features`; assigned-session resize procedure using temporary `hyprctl eval 'hl.monitor(...)'` mode changes with explicit restore | Numeric cover/crop/source-bound vectors pass; live resize reconfigures without smearing, out-of-bounds access, or scene upload | Pure vectors cover square upscale, wide downscale, odd dimensions, and portrait/landscape crop cases. Live trace presented at 1920×1080, reconfigured/presented at 1280×720, and restored/presented at 1920×1080. Before/after/restored monitor JSON confirms the temporary mode was restored | **pass** | `cargo-test.log`; `worldwall-s03-resize-clean.log`; `worldwall-s03-monitors-*.json` |
| **S03-AC4** | Package-loader bootstrap with `WORLDWALL_SCENE_DIR=/tmp/worldwall-s03-package`; virtual-input procedure against ordinary Alacritty; `tools/run_nested_s01.sh` | Textured background remains input-free, resizes, exits cleanly; static scene uploads only at load/recovery | Package path decoded the 3×2 synthetic PNG pair and presented with one upload. The ordinary Alacritty window retained the same active address/title before and after Worldwall; virtual click and `x` + Enter reached the terminal (`input-check.txt` contains `x`). Resize trace reports `scene-upload=false` on both reconfigurations. Nested real Hyprland output-removal and compositor-disconnect phases both presented the indexed scene and recorded GPU-before-layer-before-selected-output teardown | **pass** | `worldwall-s03-package-live.log`; `worldwall-s03-input-*`; `worldwall-s03-resize-clean.log`; `worldwall-s03-nested-final.log` |
| **S03-AC5** | Review of S01/S02 evidence, ADR-0003, this record, and the setup/run instructions | Assigned reviewer accepts M1 after all S03 evidence is complete | `simplysoup` approved M1 and S03-AC5 in the current acceptance instruction on 2026-09-21 after reviewing the submitted evidence; setup/run instructions describe the working indexed demo | **pass** | This record; [ADR-0003](../decisions/ADR-0003-s03-scene-decoding-color.md); [S03 stage](../stages/03-indexed-scene.md); [README](../../README.md) |

## Implementation and review notes

- `src/scene.rs` accepts only non-animated 8-bit grayscale/indexed scene PNGs and
  a 256×1 opaque RGBA8 palette, preserving raw labels and enforcing the accepted
  16 MiB encoded, 64 MiB decoder, and 16,777,216-pixel limits.
- `src/renderer.rs` uploads `R8Uint` indices once, uses integer texture loads,
  keeps palette bytes in `Rgba8Unorm`, explicitly converts authored sRGB values
  in WGSL, and requires an sRGB presentation format. Resize changes only the
  16-byte output mapping uniform. The S02 solid path remains available with
  `WORLDWALL_SOLID_COLOR=1`.
- `src/wayland_surface.rs` retains the accepted S02 display/surface lifetime
  and teardown ordering. `WORLDWALL_SCENE_DIR` is only a fixed `scene.png` /
  `palette.png` loader seam; `scene.toml`, package containment, registry, and
  selection remain later-stage work.
- The live appearance check is supplemental. The GPU readback test, protocol
  trace, input event result, resize topology records, and teardown traces are
  recorded separately; no screenshot is being used as proof of those behaviors.

## Validation commands

| Check | Command | Result |
| --- | --- | --- |
| Formatting | `cargo fmt --check` | pass |
| Rust tests | `cargo test --locked --all-targets --all-features` | 17 passed, 1 ignored GPU test |
| GPU reference | `cargo test --locked renderer::tests::gpu_indexed_reference_readback_matches_palette_and_cover_vectors -- --ignored --exact --nocapture` | pass |
| Lint | `cargo clippy --locked --all-targets --all-features -- -D warnings` | pass |
| Build | `cargo build --locked` | pass |
| Documentation checker | `python3 tools/check_docs.py` | pass; 66 Markdown files |
| Tool tests | `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/tests -v` | pass; 33 tests |
| Diff hygiene | `git diff --check` | pass |

## Limitations and next action

The live evidence is for the assigned Hyprland/Vulkan host and the retained
nested real-Hyprland harness; it does not establish universal hardware
compatibility. The GPU readback is an integration test, not a physical display
capture. No animation, registry, selection, weather, multi-output, or release
budget behavior was introduced or tested. Those remain later-stage scope.

**Next owner/action:** S04 may be planned under the roadmap after its own
stage-local animation schema decision; no S04 implementation is accepted by
this record.

## Review and acceptance

- **Reviewer / date:** `simplysoup` / 2026-09-21
- **Decision:** M1 and S03-AC5 accepted
- **Unmet criteria:** none
