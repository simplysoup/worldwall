# Evidence — S01 single-output Wayland lifecycle increment

- Date / operator: 2026-09-21 / Pi coding agent
- Revision or artifact hashes: working tree; `src/main.rs` SHA-256 `4c0570209f258f9bf32127c91cffb844fc0223c2efcb1477f0caeae46f216989`; `src/wayland_surface.rs` SHA-256 `5ff6fe02cceef54b51523ca8aa454a6d4127e6189b24f85d5e425ea3e1abd899`; `tools/run_nested_s01.sh` SHA-256 `c214c05d7d84e8b65009636e5fc942d70d484fe0888bc1e79458fa75e6bdb2a3`
- Stage / requirements: S01; WW-01, WW-02, WW-11; S01-AC1 through S01-AC4 increment
- Environment: Linux x86_64, kernel `7.2.4-zen2-1-zen`, outer Hyprland `0.56.2`, assigned Wayland session `wayland-1`; two 1920×1080 scale-1 outputs; Rust/Cargo `1.94.0`; exact locked dependencies from S00. Nested checks used a second Hyprland `0.56.2` instance with `WLR_BACKENDS=wayland`, `WLR_RENDERER=gles2`, and a separate Wayland socket; its nested output reported scale 1 and a compositor-selected 931×941 logical extent.
- Inputs: no buffer, scene, network, or GPU surface; layer-only startup; selected output is deterministic by compositor-reported position/name/global id. The nested harness removes the selected output in one run and terminates the nested compositor in a second run.
- Kind: pure state tests, locked Rust checks, live assigned-compositor startup/configure trace, live nested-Hyprland lifecycle trace

## Results

| Criterion | Exact command or manual procedure | Expected | Observed | Outcome | Artifact |
| --- | --- | --- | --- | --- | --- |
| S01-AC1 | `timeout --signal=TERM 8s cargo run --locked` in the assigned Hyprland session; `tools/run_nested_s01.sh` | Globals are discovered, one output is selected, Background role settings and bufferless initial commit are sent, then configure is acknowledged once and recorded | Both assigned-host and nested traces showed `globals-ready`, deterministic selected output, Background/all-anchor settings, `initial-commit buffer=none`, and one configure with `ack=toolkit-before-handler`; SCTK 0.21.1 dispatch acknowledges the serial before the handler callback | pass | `src/wayland_surface.rs`, `tools/run_nested_s01.sh`; live stdout from nested run retained in the evidence execution |
| S01-AC2 | Inspect `WaylandClient::install_layer` setup and the same live trace | No keyboard focus request and explicit empty pointer/touch input region are configured | Code uses `KeyboardInteractivity::None`; an empty `wl_region` is attached rather than `None` (which would restore the default infinite region); trace reports `keyboard=none input-region=empty` | pass for inspectable configuration; visible input pass-through is deferred until S02 has content | `src/wayland_surface.rs` |
| S01-AC3 | `cargo test --locked --all-targets --all-features`; `tools/run_nested_s01.sh` | State tests cover resize, integer scale, unresolved/zero extent, close, selected-output removal, and disconnect without panic or spin | Six tests passed. Nested real-Hyprland run produced `layer-closed`, `selected-output-removed`, and `compositor-disconnect` traces; both paths reached explicit owner cleanup | pass | `src/wayland_surface.rs` test module; nested harness logs |
| S01-AC4 | `cargo fmt --check`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo build --locked`; `tools/run_nested_s01.sh` | Event loop blocks on real compositor events without a spin loop and cleanup does not access destroyed owners | Build/checks passed; assigned run remained idle after configure. Nested real Hyprland runs blocked on events and then exited through output-removal and compositor-disconnect paths, logging `owners-dropped layer-before-selected-output=true` | pass for nested lifecycle evidence; assigned physical-session teardown not repeated | `src/main.rs`, `src/wayland_surface.rs`, `tools/run_nested_s01.sh` |

## Implementation

- Added `src/wayland_surface.rs` as the bounded S01 owner of the connection-facing state.
- Binds `wl_compositor`, SCTK output state, and `zwlr_layer_shell_v1`; reports missing required globals and exits cleanly.
- Selects one output deterministically after an initial roundtrip, creates a Background layer, sets all anchors, size `0x0`, exclusive zone `-1`, keyboard interactivity `None`, and an explicit empty input region.
- Commits without a buffer, records SCTK's configure/ack ordering, keeps logical and physical extents separate, and defers unresolved zero dimensions.
- Handles configure/scale/transform, selected-output removal, layer close, and dispatch errors without a render loop or GPU surface. `shutdown_owners` and the close/removal callbacks explicitly drop the layer before the selected output and registry state; no `'static` lifetime or raw-handle surface is constructed.

## Limitations and failures

The assigned-host command remains intentionally bounded with `timeout`; the repeatable close/removal/disconnect evidence comes from the nested real-Hyprland harness rather than a mock. The nested compositor uses GLES2 and a window-sized output, so this evidence does not prove Vulkan presentation, physical monitor hotplug behavior, input pass-through with visible content, or performance. No S02 `wgpu` surface exists yet.

## Review and acceptance

- Reviewer / date: `simplysoup` / 2026-09-21
- Decision: accepted by `simplysoup`; S01 is `accepted`
- Unmet criteria / stale dependent evidence: none for S01; nested GLES2 lifecycle evidence does not replace S02 Vulkan/GPU acceptance or later physical multi-output gates
- Next owner / action: assign an S02 owner and begin the safe GPU presentation stage
