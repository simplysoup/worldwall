# S04 increment — exact byte-space BlendShift

- Date / operator: 2026-09-22 / Pi coding agent
- Revision or artifact hashes: uncommitted working tree; live trace SHA-256 `294170a1545ddb487e3b544a5dbfe5c1a33c5493b0843b84a234fb14151114eb`
- Stage / requirements: S04, WW-03, WW-04, S04-AC1, S04-AC3
- Change authorization: `simplysoup` approved CR-0001 in the user instruction; CR-0001 is applied here
- Environment: assigned Wayland/Hyprland session (`WAYLAND_DISPLAY=wayland-1`), 1920×1080 scale 1, NVIDIA GeForce RTX 3070 Ti, Vulkan/wgpu
- Inputs: local untracked `fixtures/untracked/effectgames/v29-seascape`, 30 FPS, 20-second deadline; no network/weather input
- Kind: unit, ignored GPU readback, and live compositor/GPU trace

## Results

| Criterion | Exact command or manual procedure | Expected | Observed | Outcome | Artifact |
| --- | --- | --- | --- | --- | --- |
| S04-AC1 / WW-04 | `cargo test --locked --all-targets --all-features` | Existing effects remain covered and BlendShift byte-space vectors pass | 63 passed; BlendShift integer rotation, preceding-member direction, 1/100 fractional fade, and byte flooring test passed; 2 pre-existing GPU tests remained ignored | pass | `src/animation.rs` tests |
| S04-AC1 / WW-04 | `cargo test --locked renderer::tests::gpu_animated_palette_readback_matches_cpu_reference_vectors -- --ignored --nocapture` | GPU shader output agrees with CPU references within one byte | 1 passed, including a `blend_shift` group in the GPU fixture | pass | renderer test output |
| S04-AC3 / WW-03 | Code review of `src/animation.rs` and `src/renderer.rs` | BlendShift uses immutable metadata/cycle lists and only the clock uniform changes per frame | Effect discriminator 4 uses the existing metadata/list buffers; no CPU pixel or full-scene upload was added | pass | source review |
| S04-AC2 / S04-AC3 | `WORLDWALL_SCENE_DIR=fixtures/untracked/effectgames/v29-seascape WORLDWALL_FPS=30 WORLDWALL_TEST_EXIT_AFTER_MS=20000 cargo run --locked` | Live GPU presentation remains paced, clean, and upload-stable | 598 presented frames, 597 animation frames, one scene upload, 598 animation uniform updates, 1920×1080 `Rgba8UnormSrgb`, clean deadline exit, owner-safe teardown | pass | [`2026-09-22-s04-byte-blendshift-live.log`](../evidence/2026-09-22-s04-byte-blendshift-live.log) |
| S04 / documentation and release checks | `cargo fmt --check`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo build --locked`; `python3 tools/check_docs.py`; `python3 -m unittest discover -s tools/tests -v` | Required code and documentation checks pass | All commands passed; documentation checker reported 81 Markdown files and archive intact; docs tests reported 33 passed | pass | command output |

## Implementation

`blend_shift` is additive to the accepted linear-RGB `palette_cycle` effect.
The shader reconstructs the authoritative RGBA8 palette bytes, applies the
reference rotation toward the preceding member, quantizes the fractional
amount to the reference precision of 1/100, floors each byte-channel fade,
and decodes the result for the sRGB presentation target. The local V29 fixture
uses the recovered source ranges and converts the source step rates to full
range cycles per second, avoiding the previous sub-frame loop speed.

## Limitations and failures

The live trace proves GPU compilation, presentation, pacing, and teardown on
the assigned host; it does not by itself prove a human visual comparison to
the external reference or geometric wave displacement. The fixture and source
art remain local untracked provenance data and are not licensed for bundling.
The full clippy/build/docs command set is recorded separately after this
increment. Real suspend/resume integration remains S08 scope.

## Review and acceptance

- Reviewer / date: `simplysoup` approved CR-0001 on 2026-09-22; stage-gate review of this implementation evidence is not separately recorded
- Decision: CR-0001 applied; S04 additive implementation evidence submitted, not a new stage acceptance claim
- Unmet criteria / stale dependent evidence: no known unmet criterion for this increment; prior S04 acceptance evidence remains historical for unchanged effects
- Next owner / action: `simplysoup` reviews the implementation/evidence and records any requested follow-up; visual reference comparison remains observational
