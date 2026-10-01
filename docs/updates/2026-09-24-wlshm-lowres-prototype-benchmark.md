# 2026-09-24 — Low-resolution `wl_shm` prototype benchmark

**Type:** exploratory implementation/measurement spike only. This does not
change the approved Vulkan baseline, accept or invalidate a stage, or request a
CR. The production Worldwall service, wallpaper config, and session config were
left untouched.

- Source baseline: Git revision `fe42865000d71802231bd64baed12ca8e4e247ce`;
  working tree contains only this prototype and its record.
- Prototype source SHA-256:
  `d074a65c79a4185bda6e8d8a30bb4b38ae0704a743800d959f7ab8d12baaad51`.
- Release example SHA-256:
  `4b3b21914d5900bc83862ec2de422ff2571a9232bf27eb8c9e4d49b7d0ebb736`.

## Prototype

Added [`examples/wlshm_lowres_bench.rs`](../../examples/wlshm_lowres_bench.rs),
a temporary-use XDG toplevel demo using SCTK `wl_shm` buffers and
`wp_viewporter`; it creates no wgpu/Vulkan renderer. It uses a synthetic
640×480 indexed source, cover-crops it to a 640×360 XRGB8888 shared-memory
buffer, and asks the compositor to scale that buffer to a maximized demo window.
The animated palette group is confined to a 320×180 rectangle (25% of the
buffer). `--mode full` rewrites/damages the whole buffer each frame;
`--mode dirty` rewrites/damages only that rectangle after each buffer has been
initialized. The benchmark is bounded and closes the demo window after warmup
and sampling or when the window is closed.

## Live environment and procedure

- Assigned desktop: Linux `7.2.6-zen2-1-zen`, Hyprland `0.56.2`, two 1920×1080
  60-Hz outputs at scale 1, NVIDIA RTX 3070 Ti / driver `615.71.09`.
- The existing `worldwall-session` stayed running throughout. Each test created
  one normal maximized XDG window; the buffer was 640×360 and the final
  configured content area was 1876×950. No screenshot or manual pixel-art
  quality review was recorded.
- Commands (run sequentially, optimized locked build):

  ```sh
  cargo build --offline --locked --release --example wlshm_lowres_bench
  target/release/examples/wlshm_lowres_bench --mode full --warmup 3 --seconds 30 --fps 30
  target/release/examples/wlshm_lowres_bench --mode dirty --warmup 3 --seconds 30 --fps 30
  ```

- A read-only sampler read `/proc/<pid>/stat`, `smaps_rollup`, and `maps`
  approximately every 0.5 seconds. CPU is percent of one core. The process
  maps contained no Vulkan- or NVIDIA-named mapping during either run; `ldd`
  showed no Vulkan, NVIDIA, GL, or EGL dependency (the Wayland client library
  was present).

## Results

| Mode | Measured submits | Mean draw-loop CPU time | Process CPU (one core) | Pixel bytes written | Write rate | RSS / PSS sampled |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Full-buffer | 900 / 30.00 FPS | 1,542.3 µs/frame | 5.15% | 829,440,000 / 30 s | 27.65 MB/s (26.37 MiB/s) | 4.22 / 1.75 MiB |
| Dirty rectangle | 904 / 30.13 FPS | 395.1 µs/frame | 1.82% | 208,281,600 / 30 s | 6.94 MB/s (6.62 MiB/s) | 4.20 / 1.71 MiB |

The dirty run had extra xdg configure/resize redraws, which accounts for its
slightly-over-30 submitted-frame mean; both runs reported zero unavailable
buffer skips. At 640×360, dirty rendering wrote exactly one quarter of the full
buffer bytes. For two identical outputs, simply doubling these per-output write
rates gives about 55.3 MB/s full-buffer or 13.9 MB/s for this 25%-dirty case;
that is arithmetic projection, **not a measured dual-output result**.

For context, a separate read-only snapshot of the already-running production
Worldwall process immediately before the prototype showed `smaps_rollup`
RSS 308,876 kB and PSS 153,297 kB. That is not an apples-to-apples comparison:
the demo is a minimal executable that omits Worldwall's registry, environment,
weather, IPC, and power-service runtime, and it ran alongside both the
compositor and the existing Worldwall process. Therefore its ~1.7 MiB PSS does
not quantify production memory savings from removing Vulkan. It does establish
that this demo process itself initialized and presented shared-memory buffers
without mapping Vulkan/NVIDIA graphics libraries.

## Validation and limitations

- `cargo test --offline --locked --example wlshm_lowres_bench`: 3 passed (cover
  viewport geometry, palette-group isolation, XRGB byte order).
- `cargo check --offline --locked --example wlshm_lowres_bench`: passed.
- `cargo clippy --offline --locked --example wlshm_lowres_bench -- -D warnings`:
  passed.
- `cargo build --offline --locked --release --example wlshm_lowres_bench`:
  passed; `ldd`/live maps checks above found no GPU backend/driver mappings.
- `git diff --check`: passed.

This prototype is a synthetic normal-window renderer, not a production layer
surface. It does not establish visible color fidelity, dirty-region processing
inside Hyprland, two-output lifecycle/CPU cost, scene-specific animated-pixel
coverage, or production PSS savings. Wayland damage requests may not guarantee
that a compositor performs only partial internal work. VRAM impact remains
unmeasured and is intentionally deferred. The example is an exploratory spike,
not acceptance evidence for WW-02/03/04 or S02/S03/S04/S09/S11.
