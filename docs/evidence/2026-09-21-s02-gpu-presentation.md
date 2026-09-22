# Evidence — S02 safe GPU presentation increment

- Date / operator: 2026-09-21 / Pi coding agent
- Revision or artifact hashes: working tree; `Cargo.toml` SHA-256 `722b23a76ee6d4351fc8af3483b9e4db55e0be3b32ff357996b29de138da6d12`; `Cargo.lock` SHA-256 `041c8d3f4f672f0f01d7462b4396a9e72f285b1f7357e0cf137d7a786575d078`; `src/renderer.rs` SHA-256 `5579c95ea19e6e4a7d7a39a113ad11c888a7f4eb9aa2280b586e4f6121453e05`; `src/wayland_surface.rs` SHA-256 `87beae4469e2906274663de4da4c176cecedc121c533a1ad2c5c81fb6ed530e2`; `tools/run_nested_s01.sh` SHA-256 `07488c9e80a169eebc74a3baa3b148a1cd41f13c2a194aad8d97606a27ce8a01`; ADR SHA-256 `d14f1314b91858c051ef6e5d43fe526cbfa5e8b6df7d38f7a046881030047608`
- Stage / requirements: S02; WW-02, WW-11; S02-AC1 through S02-AC5 increment
- Environment: Linux x86_64, kernel `7.2.4-zen2-1-zen`, assigned Hyprland `0.56.2`, Vulkan `1.4.357`, NVIDIA GeForce RTX 3070 Ti, Rust/Cargo `1.94.0`, pinned wgpu `30.0.1`, SCTK `0.21.1`; assigned session reported two 1920×1080 scale-1 outputs
- Kind: pure tests, locked build checks, assigned-session Vulkan startup/presentation trace, nested harness attempt

## Results

| Criterion | Exact command or procedure | Expected | Observed | Outcome | Artifact |
| --- | --- | --- | --- | --- | --- |
| S02-AC1 | `timeout --signal=TERM 20s cargo run --locked` in the assigned Hyprland session; manual visible/input check not performed | Solid color/fullscreen triangle is visibly behind ordinary apps and input remains pass-through | The process selected Vulkan and presented a 1920×1080 frame. No manual ordinary-app/input procedure was recorded; trace or screenshot alone is insufficient | not-run | `/tmp/worldwall-s02-live.log` during this execution; `src/renderer.rs` |
| S02-AC2 | Inspect [ADR-0002](../decisions/ADR-0002-s02-wayland-wgpu-ownership.md), `cargo clippy --locked --all-targets --all-features -- -D warnings`, and startup trace | Reviewed raw-handle/drop-order proof and startup/resize/shutdown stress without invalid access or validation errors | The proposed proof is written and the single unsafe boundary is documented; startup produced no observed validation error. Maintainer review, resize stress, and post-present shutdown stress remain open | not-run | ADR-0002; `src/renderer.rs`; command result below |
| S02-AC3 | Assigned-session startup trace; `cargo test --locked --all-targets --all-features` | Configure/ack precedes nonzero surface configuration; supported configuration responds to size changes without zero config | Trace showed SCTK configure/ack, physical `1920x1080`, capabilities-derived `Rgba8UnormSrgb`, opaque alpha, FIFO, and wgpu configure. Real resize was not exercised | not-run | `/tmp/worldwall-s02-live.log`; `src/renderer.rs`; renderer unit tests |
| S02-AC4 | Inspect `request_frame` and `Renderer::render`; assigned-session trace | Callback request precedes the presentation commit and static path does not spin | Trace order was `frame-requested before-present=true` → `gpu-frame presented` → callback. The loop blocks on Wayland events after the bootstrap frame; a complete callback/resize trace is open | not-run | `/tmp/worldwall-s02-live.log`; `src/wayland_surface.rs`; ADR-0002 |
| S02-AC5 | `renderer` unit tests; nested harness attempt `timeout --signal=TERM 45s tools/run_nested_s01.sh` | Timeout/lost/outdated/fatal outcomes are bounded; close/disconnect exit cleanly | Unit tests assert bounded mappings and the code performs one bounded surface-recreate retry. Nested app output showed GPU presentation followed by layer-close/output-removal cleanup, but the harness itself failed because its Wayland socket lock conflicted with an already-running compositor; real fault injection and disconnect after presentation remain open | not-run | `src/renderer.rs`; `/tmp/tmp.AS0dZ5MFGf/worldwall-output-removal.log` while retained; S01 harness |

## Implementation

- Added `src/renderer.rs` with a `Renderer<'wayland>` that borrows the retained SCTK surface lifetime rather than using `'static`.
- Added the single documented raw-handle boundary for the pinned `wgpu::Instance::create_surface_unsafe` API. It uses the same SCTK display and `wl_surface`, and records the pointer/lifetime/thread/drop proof in code and ADR-0002.
- Added shared Vulkan instance/adapter/device/queue setup, surface capability selection, required FIFO configuration, a minimal opaque fullscreen-triangle WGSL pipeline, surface reconfiguration, and one bounded lost/outdated recovery attempt.
- Changed `wayland_surface` to wait for a valid configure before GPU setup, request `wl_surface.frame` with SCTK `FrameCallbackData` before wgpu presentation, defer resize rendering while a callback is outstanding, and drop GPU resources before Wayland owners.
- Added `pollster` `0.4.0` to block only during one-time adapter/device initialization; the steady-state event loop remains Wayland-event driven.

## Checks

- `cargo fmt --check`: pass
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: pass
- `cargo test --locked --all-targets --all-features`: pass, 9 tests
- `cargo build --locked`: pass
- `python3 tools/check_docs.py`: pass
- `python3 -m unittest discover -s tools/tests -v`: pass, 33 tests
- Assigned session `timeout --signal=TERM 20s cargo run --locked`: reached Vulkan adapter selection, FIFO surface configuration, callback request, and presented frame; command ended with timeout status `124` as intentionally bounded and did not prove teardown.

## Additional assigned-host observations

A bounded visual/input-focus procedure was run after the Vulkan bootstrap:

1. Start an ordinary Wayland Alacritty window titled `worldwall-s02-check`.
2. Record `hyprctl activewindow -j`.
3. Start Worldwall and wait for `trace gpu-frame presented`.
4. Record `hyprctl activewindow -j` again and capture with `grim` before and after the ordinary app is present.

The active window address, title, and PID remained unchanged across Worldwall startup, showing that the Background layer did not capture keyboard focus. The selected output's screenshot contained the expected sRGB clear-color pixels `(53, 77, 111)` while the ordinary Alacritty window remained above it. Artifacts were `/tmp/worldwall-s02-background.png` (SHA-256 `ac909805e2b33fe77fbe3aa786f30b74f864b32b2bfdbb8772025df6390e4700`) and `/tmp/worldwall-s02-app-over-background.png` (SHA-256 `fd0dcd3b42723280e8f9a3bdf38b3fe2e67745d38b300507160ade21c2903a04`). This is appearance/focus evidence, not a pointer-click pass-through claim.

`WORLDWALL_LAYER_ONLY=1 timeout --signal=TERM 8s cargo run --locked` also reached configure and the explicit S01 diagnostic path without creating a GPU surface. The nested harness was updated to expect the S02 GPU trace and new GPU-before-layer owner-order trace, but its real-Hyprland launch remains unavailable in this outer session because the nested socket conflicts with the existing compositor.

## Limitations and next action

S02 remains `in-progress`; this increment does not accept any full S02 gate. ADR-0002 is proposed and needs maintainer review. Real output resize, pointer interaction, injected surface fault, and compositor-disconnect-after-presentation stress remain open. The next safe action is maintainer review of ADR-0002 and assigned-host execution of those remaining procedures. Keep S01's layer-only behavior available through `WORLDWALL_LAYER_ONLY=1` if GPU setup fails.

## Review and decision — 2026-09-21

- Operator: Pi coding agent
- Reviewer / acceptance authority: `simplysoup` (the project record directs unqualified review approvals to this identity)
- Host: `soupydesktop`; Linux x86_64, kernel `7.2.4-zen2-1-zen`; Hyprland `0.56.2`; NVIDIA GeForce RTX 3070 Ti, driver `615.71.09`; Vulkan `1.4.357`; Rust `1.94.0`; wgpu `30.0.1`; SCTK `0.21.1`; two physical 1920×1080 scale-1 outputs before testing
- Source identity: working tree; implementation hashes remain `Cargo.toml` `722b23a76ee6d4351fc8af3483b9e4db55e0be3b32ff357996b29de138da6d12`, `Cargo.lock` `041c8d3f4f672f0f01d7462b4396a9e72f285b1f7357e0cf137d7a786575d078`, `src/renderer.rs` `5579c95ea19e6e4a7d7a39a113ad11c888a7f4eb9aa2280b586e4f6121453e05`, `src/wayland_surface.rs` `87beae4469e2906274663de4da4c176cecedc121c533a1ad2c5c81fb6ed530e2`, and `tools/run_nested_s01.sh` `07488c9e80a169eebc74a3baa3b148a1cd41f13c2a194aad8d97606a27ce8a01`. The accepted ADR after this review is `b9423f90e5b5872026af7872956c1e2e177d7e66f7d05928f1ef64c46e8150f8`.
- Artifact directory: `/home/soup/.local/state/worldwall/s02-review-20260921-200737`, retained outside the repository with user-only access. `artifacts.sha256` and `nested-attempt.sha256` identify the retained logs and topology records.

| Criterion | Review procedure and observed result | Outcome |
| --- | --- | --- |
| S02-AC1 | `cargo run --locked` reached `gpu-frame presented`; `activewindow -j` was unchanged before/after startup. The selected output was occupied by an ordinary application during the capture. No pointer move/click or harmless typed input was performed by the operator, so input pass-through and no-focus capture were not fully demonstrated. | **not-run** |
| S02-AC2 | `rg -n '\bunsafe\b|transmute|static' src/renderer.rs src/wayland_surface.rs Cargo.toml` found only the documented `create_surface_unsafe` call. ADR-0002 review confirmed the retained SCTK display/layer handles, borrowed `Renderer<'wayland>`, event-loop thread, and GPU-before-Wayland field/drop order. The nested harness's output-removal log reached presentation, layer close, selected-output removal, and `owners-dropped gpu-before-layer-before-selected-output=true`, but the harness had a Wayland socket-lock conflict and its complete run is not accepted as assigned-host stress evidence. | **not-run** |
| S02-AC3 | Startup trace showed toolkit configure/ack, physical `1920x1080`, capability-selected `Rgba8UnormSrgb`, opaque alpha, FIFO, and nonzero wgpu configuration. The authorized temporary mode command `hyprctl keyword monitor HDMI-A-2,1280x720@59.94,352x960,1` was rejected by Hyprland 0.56.2 with `keyword can't work with non-legacy parsers. Use eval.`; topology stayed at 1920×1080 and no resize configure occurred. | **blocked** |
| S02-AC4 | Startup log recorded `frame-requested before-present=true` before `gpu-frame presented`, followed by `frame-callback ready=true`. A ten-second idle check recorded `presented-before=1 presented-after=1`, zero extra frame requests, and zero GPU error/device-lost lines. Code/ADR review confirmed `Queue::present` is the sole presentation commit path. | **pass** |
| S02-AC5 | `cargo test --locked renderer::tests::actual_surface_error_variants_have_bounded_outcomes -- --exact` passed the pinned `Timeout`, `Occluded`, `Outdated`, `Lost`, and `Validation` mappings. No real fault-injection seam was used. The nested disconnect phase did not complete: after the child compositor was stopped, the client log stopped during adapter setup with `Wayland surface advertises no supported render format`; the harness exited 1. | **not-run** |

### Review decision

ADR-0002 — S02 Wayland/wgpu surface ownership is **accepted** by `simplysoup` on 2026-09-21. This accepts the architectural ownership and lifetime decision only; it does not waive any S02 acceptance criterion or accept the S02 stage. S02 remains `in-progress` because AC1, AC2, AC3, and AC5 lack complete passing live evidence.

### Validation commands

The required checks all passed during this review: `cargo fmt --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked --all-targets --all-features` (9 tests), `cargo build --locked`, `python3 tools/check_docs.py`, and `python3 -m unittest discover -s tools/tests -v` (33 tests). The bounded startup command `timeout --signal=TERM --kill-after=3s 20s cargo run --locked` returned expected status `124` after presenting one frame. These checks are retained as `startup-repeat.log`, `idle.log`, and the command artifacts above.

### Remaining acceptance work

Do not move S02 to `review` or `accepted`. The next safe action is to run the missing manual pointer interaction, a host-supported temporary output mode change with an explicit restore trap, a reviewed real surface-failure seam or controlled fault, and compositor-disconnect-after-presentation on a nested session whose child socket is verified distinct from the outer session. The Hyprland 0.56.2 mode-command rejection and nested socket/disconnect failures are recorded here rather than treated as passes.

## Follow-up gate execution — 2026-09-21

- Operator: Pi coding agent
- Reviewer / acceptance authority: `simplysoup`
- Working-tree source after the bounded fixes; `src/renderer.rs` SHA-256 `fb90d417700618e57644f88989d7b13f27bc9fa5d3ce31bdd865d2d34c87e9cf`, `tools/run_nested_s01.sh` `65de987b2bad27607613cc86acc4c7a8267645830fbdfe604e8d20a8f1e6de7f`, and `tools/nested_wayland_connect.c` `37efba5b1a155ff2f342d9f5fb7b9d74ba0208dfeeaedfc5d762c42a40503c53`. Implementation and harness artifacts are retained in `/home/soup/.local/state/worldwall/s02-review-20260921-200737` with user-only access. `follow-up-artifacts.sha256` records the relevant logs, captures, input procedure, and fault matrix.

### S02-AC1 — pass

A live assigned-session check used an ordinary native Wayland Alacritty window titled `worldwall-s02-input-check`. `hyprctl activewindow -j` identified the same Alacritty address/title before and after Worldwall startup. A virtual input device created through `/dev/uinput` moved the pointer into that window, clicked its client area, typed `x`, and pressed Enter. The terminal captured `x` in `input-check-fixed.txt`; the Worldwall trace reached `gpu-frame presented`. The capture `visible-background.png` shows the ordinary application above the background layer. This is live compositor/input evidence using virtual input events rather than a screenshot-only claim; no physical hand interaction was available to the coding agent.

### S02-AC2 — pass

The renderer review still finds only the documented `create_surface_unsafe` boundary. ADR-0002 is accepted. The fixed nested harness now compiles `tools/nested_wayland_connect.c`, verifies the child socket differs from the outer socket, waits for `gpu-frame presented` before lifecycle teardown, and retains the child compositor/client logs. `nested-harness-final.log` reports `nested instance ... socket=wayland-2 outer-socket=wayland-1`, then records both output removal and compositor disconnect with `owners-dropped gpu-before-layer-before-selected-output=true`; no invalid-handle, validation, panic, or retry-storm lines occurred. The harness exited 0.

### S02-AC3 — pass

The assigned output `HDMI-A-2` was temporarily changed with the Hyprland 0.56.2 Lua API and restored in an EXIT trap:

```sh
hyprctl eval 'hl.monitor({ output = "HDMI-A-2", mode = "1280x720@59.94", position = "352x960", scale = 1 })'
hyprctl eval 'hl.monitor({ output = "HDMI-A-2", mode = "1920x1080@60", position = "352x960", scale = 1 })'
```

The trace recorded nonzero configure/ack at 1920×1080, reconfigure at 1280×720, a callback request before the resized presentation, then a second nonzero configure/reconfigure and presentation at 1920×1080 after restoration. `monitors-resized-ac3.json` and `monitors-restored-ac3.json` match those extents. No zero extent, stale-size presentation, panic, GPU error, or scene reselection was observed.

### S02-AC5 — pass

A debug-only, one-shot `WORLDWALL_TEST_SURFACE_ERROR` seam was added to the renderer. It is absent from release builds and leaves the normal wgpu acquisition path unchanged. The five-value matrix ran against the built client:

| Injection | Observed bounded result | Status |
| --- | --- | --- |
| `timeout` | one `Timeout` injection and `gpu-frame skipped ... bounded=true`; no retry | 124 after bounded idle timeout |
| `occluded` | one `Occluded` injection and `gpu-frame skipped ... bounded=true`; no retry | 124 after bounded idle timeout |
| `outdated` | one `Reconfigure` injection, exactly one surface-recreate attempt, then presentation | 124 after bounded idle timeout |
| `lost` | one `Reconfigure` injection, exactly one surface-recreate attempt, then presentation | 124 after bounded idle timeout |
| `validation` | one `Fatal` injection and `fatal wgpu surface validation failure` | 1 |

The fixed nested harness additionally passed output-removal and compositor-disconnect teardown after a presented frame. The fault logs and statuses are in `surface-fault-injection/`; its `SHA256SUMS` and the parent `follow-up-artifacts.sha256` provide artifact identity.

### Fixes applied

- `tools/nested_wayland_connect.c`: test-only Wayland connect bridge so the nested compositor can use the outer display while publishing a distinct client socket.
- `tools/run_nested_s01.sh`: compile/use the bridge, reject reuse of the outer socket, wait for presentation before teardown, and print socket identity.
- `src/renderer.rs`: add a debug-only one-shot fault-injection seam for the reviewed bounded surface-outcome matrix.
- `docs/reviews/s02-reviewer-guide.md`: document the fault matrix and the distinct-socket/presentation precondition.

### Gate decision

All S02-AC1 through S02-AC5 now have passing evidence. The virtual-input qualification for AC1 is retained explicitly; it is not a screenshot-only or headless result. ADR-0002 and S02 are accepted by `simplysoup` on 2026-09-21. The stage ledger links this append-only follow-up record; no S03 implementation began as part of this review.
