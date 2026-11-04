# 2026-09-21 — S04 palette animation and fixture validation increment

**Type:** bounded implementation increment and stage acceptance record;
S04 is accepted.

**Operator:** Pi coding agent.  **Reviewer / acceptance authority:**
`simplysoup` / 2026-09-21.

**Working-tree source identity:** `src/animation.rs` SHA-256
`60f61b522dc65b5c407a1dc45097efc5a118d84272b61b2a56d52d45a98e0674`,
`src/scheduler.rs` `3035e456f8ad17439530bd29f09466d5ab719d460a5ae3a2d9417e14ad3f7438`,
`src/renderer.rs` `67e87f1b909c9f23b6299fdda8df5114f2aa60c9d9425ba3d3abf03188f2594d`,
`src/scene.rs` `8225628a80e87b5025703b0fc1f63730bcfc8d02b26040840e825536d5d75f26`,
`src/wayland_surface.rs` `c0b43b742e520f93461980e07140bbf742abcd9ad68fefea70cfd783a9b478dd`.

**Scope:** S04; WW-03, WW-04, WW-05, WW-11; animation schema/packing,
GPU palette effects, bounded monotonic animation clock, and callback/deadline
pacing. S03's Wayland ownership and indexed-resource boundaries are retained.

## Outcome

- Added `src/animation.rs` with strict dependency-free `animation.toml`
  parsing, finite/range/overlap validation, ordered range/list groups, CPU
  reference vectors, and immutable 32-byte-per-entry metadata plus cycle-list
  packing.
- Added `src/scheduler.rs` with fake-clock nanosecond tests for bootstrap,
  callback AND deadline gating, 15/30 FPS intervals, withheld callbacks,
  delayed callbacks without catch-up bursts, and resize bootstrap.
- Added a debug-only clean-deadline exit and callback-withholding seam in
  `src/wayland_surface.rs` for bounded live lifecycle evidence. The seams are
  absent from release builds and do not alter the normal callback path.
- Added `tools/measure_s04.sh` to collect the prescribed 60-second warmup and
  three 5-minute CPU/RSS/GPU/pacing samples for each single-output workload.
- Extended `Scene` and the fixed package loader to accept optional
  `animation.toml` while preserving static behavior when absent.
- Extended the indexed renderer with immutable metadata/cycle storage buffers
  and a 16-byte animation clock uniform. Hue cycling, brightness pulsing, and
  ordered palette-cycle interpolation run in linear RGB in WGSL. Normal frames
  write only the clock uniform; scene upload count remains one.
- Added an ignored assigned-GPU readback comparison covering all three animated
  effects at multiple phases, including the bounded-clock edge, with a
  one-sRGB-byte RGB tolerance and exact alpha check.
- Materialized the fetched V26, V19, and V29 palette/pixel sources as local
  untracked PNG/source fixtures under `fixtures/untracked/effectgames/` and
  exercised the V29 package through the real Wayland/Vulkan path.
- Replaced the blocking dispatch loop with a calloop Wayland source plus a
  deadline timer. The scheduler still requires a compositor frame callback for
  steady-state frames; the initial frame is an explicit bootstrap path.
- Added [ADR-0004](../decisions/ADR-0004-s04-animation.md) and an example
  schema to the README. `simplysoup` approved the decision on 2026-09-21;
  the implementation evidence is now submitted for stage review.

## Validation

| Check | Exact command / procedure | Result | Outcome |
| --- | --- | --- | --- |
| Pure/unit | `cargo test --locked --all-targets --all-features` | 32 tests: 30 passed, 2 ignored assigned-GPU tests. Animation tests cover static, hue, pulse, ordered cycle, malformed/overlap/duplicate metadata, signed speed, and 1024-second phase wrap; scheduler tests cover fake-clock pacing cases | pass |
| Lint | `cargo clippy --locked --all-targets --all-features -- -D warnings` | no warnings | pass |
| Build | `cargo build --locked` | completed | pass |
| Static GPU regression | `cargo test --locked renderer::tests::gpu_indexed_reference_readback_matches_palette_and_cover_vectors -- --ignored --exact --nocapture` | Vulkan readback passed after the new storage/uniform bindings were compiled and exercised with a static scene | pass |
| Live 30 FPS bootstrap/pacing | `timeout --signal=TERM --kill-after=3s 8s cargo run --locked`; retained log `$HOME/.local/state/worldwall/s04-implementation-20260921/wayland-30fps.log` | assigned Vulkan/Hyprland session configured 1920×1080, bootstrapped, then emitted 231 animation frames with phase deltas approximately 33.3–34.8 ms; scene upload stayed at 1 and clock updates increased per frame. Timeout 124 was intentional process bounding, not a clean-shutdown result | pass for bounded pacing observation; not sustained acceptance |
| Animated GPU readback | `cargo test --locked renderer::tests::gpu_animated_palette_readback_matches_cpu_reference_vectors -- --ignored --exact --nocapture` | Vulkan readback compared hue-cycle, brightness-pulse, and ordered palette-cycle output at four phases, including 1023.75 seconds, against CPU reference vectors; RGB tolerance was 1 sRGB byte and alpha was exact | pass |
| Live 15 FPS pacing | `WORLDWALL_SCENE_DIR=fixtures/untracked/effectgames/v29-seascape WORLDWALL_FPS=15 WORLDWALL_TEST_EXIT_AFTER_MS=5000 cargo run --locked` | 75 animated frames from the fetched V29 indexed fixture; phase deltas approximately 66.7 ms, `scene-upload-count=1`, and clean owner teardown; log `$HOME/.local/state/worldwall/s04-implementation-20260921/wayland-v29-clean-15fps.log` | pass for bounded live pacing and teardown |
| Live 30 FPS package path | `WORLDWALL_SCENE_DIR=fixtures/untracked/effectgames/v29-seascape WORLDWALL_FPS=30 WORLDWALL_TEST_EXIT_AFTER_MS=5000 cargo run --locked` | 150 animated frames from the fetched V29 indexed fixture; phase deltas approximately 33.4 ms, `scene-upload-count=1`, and clean owner teardown; log `$HOME/.local/state/worldwall/s04-implementation-20260921/wayland-v29-clean-30fps.log` | pass for bounded live pacing and teardown |
| Live callback withheld | Same package with `WORLDWALL_TEST_WITHHOLD_CALLBACK=1` and a 3-second debug deadline | Bootstrap presented once, the compositor callback was explicitly withheld by the debug seam, no further frame was submitted, and clean teardown completed; log `$HOME/.local/state/worldwall/s04-implementation-20260921/wayland-v29-withheld-callback-clean.log` | pass for bounded callback-gate evidence; seam is not a compositor-occlusion claim |
| Live occluded outcome | Same package with `WORLDWALL_TEST_SURFACE_ERROR=occluded` and a 3-second debug deadline | One bounded `Occluded` outcome, no retry loop, and clean teardown; log `$HOME/.local/state/worldwall/s04-implementation-20260921/wayland-v29-occluded-clean.log` | pass for bounded fault-path evidence |
| Live scale/resize | Assigned Hyprland `hl.monitor` scale 1→2→1 perturbation and 1920×1080→1280×720→1920×1080 mode perturbation with the V29 package and debug deadlines | Both real configure sequences reached the renderer; scale reconfigure logged `scene-upload=false`, and mode reconfigure presented at 1280×720 then restored 1920×1080 with `scene-upload=false`. Both runs tore down cleanly. Logs: `$HOME/.local/state/worldwall/s04-implementation-20260921/wayland-v29-scale-clean.log`, `$HOME/.local/state/worldwall/s04-implementation-20260921/wayland-v29-mode-resize-clean.log`; snapshots: `scale-*.json`, `mode-resize-*.json` | pass for live resize/scale and no-reupload observation |
| Resource baseline | `tools/measure_s04.sh` runner; 60-second warmup plus three approximately 5-minute samples for static/animated V29 at 15/30 FPS; source fixture checksums in `$HOME/.local/state/worldwall/s04-implementation-20260921/resource-baseline.sha256` | All four workloads completed with clean teardown. Sample durations were 298.4–299.4 s. CPU averages were 4.8–5.3% at 15 FPS and 8.6–8.9% at 30 FPS; RSS maxima were 216,084–220,844 KiB; GPU utilization averages were 8.9–14.2%; measured power averaged 30.58–31.18 W; VRAM was 1186 MiB; trace frame deltas stayed near 66.7 ms or 33.3 ms with documented delayed samples; every trace kept `scene-upload-count=1` | pass as an observational S04 baseline; no release limits or energy claim |
| Documentation/tooling | `python3 tools/check_docs.py`; `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/tests -v` | checker: 68 Markdown files, 12 stages, 7 skills; 33 tooling tests passed | pass |

Raw trace checksums are retained in `$HOME/.local/state/worldwall/s04-implementation-20260921/artifacts.sha256`.

## Approval and external example-source inspection

`simplysoup` approved ADR-0004 on 2026-09-21. The approval covers the exact
fields, effect semantics, linear-RGB operations, metadata packing, bounded
clock, and validation ranges. After reviewing the implementation evidence
below, `simplysoup` accepted S04 on 2026-09-21.

At the user's request, the public catalog and three example scene scripts were
fetched from <https://www.effectgames.com/demos/worlds/> for provenance
inspection. The scripts are embedded 640×480 LBM palette/pixel sources:

- V26 winter forest: `cache/V26-01January-janclrscpt.js`, 1,300,251 bytes
- V19 mountain stream: `cache/V19-02February-febclrscpt.js`, 1,163,312 bytes
- V29 seascape: `cache/V29-09September-SEPTCLRCUMSCPT.js`, 1,096,512 bytes

The source URLs and metadata are retained at
`$HOME/.local/state/worldwall/s04-implementation-20260921/effectgames-example-sources.json`.
The derived PNGs and raw responses are deliberately local untracked fixtures
under `fixtures/untracked/effectgames/`; they are not part of the tracked
commit or published delivery. No license or redistribution permission was
inferred.

## Unmet gates and limitations

- The four-workload S04 resource baseline is complete. It is observational:
  CPU/GPU/RSS/power limits are not approved, and no energy claim is made.
  Raw samples, traces, and checksums are retained under
  `$HOME/.local/state/worldwall/s04-implementation-20260921/`.
- The callback-withholding and occluded runs use debug-only seams for bounded
  failure-path evidence; they are not claims that every compositor suppresses
  callbacks when a wallpaper is covered.
- The Effect Games-derived PNGs and raw source snapshots are local untracked
  inspection fixtures only. They are not part of the tracked delivery and
  must not be published or committed without license review. The default
  scene remains static and synthetic.

## Next safe action

S04 is accepted. S05 may begin its registry and selection work while preserving
the accepted indexed-renderer, animation, and pacing boundaries.

## Review and acceptance

- Reviewer / date: `simplysoup` / 2026-09-21
- Decision: S04 accepted after review of ADR-0004, implementation, GPU
  readback, live lifecycle, pacing, resize, and resource-baseline evidence
- Unmet criteria: none
