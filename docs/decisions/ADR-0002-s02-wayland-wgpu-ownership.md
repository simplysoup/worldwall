# ADR-0002 — S02 Wayland/wgpu surface ownership

- Status: accepted
- Owner / date: Pi coding agent / 2026-09-21
- Approval identity / date: `simplysoup` / 2026-09-21
- Affected stages / requirements: S02, WW-02, WW-11
- Related change proposal: none

## Context and constraints

S02 must attach wgpu 30.0.1 to the exact `wl_surface` created by SCTK 0.21.1. The pinned wgpu API exposes raw Wayland creation through `Instance::create_surface_unsafe`; its safety contract requires the display and surface handles to remain valid until the returned `Surface` is dropped. The project must not manufacture a `'static` owner, leak a proxy, or hide a destruction-order defect.

The existing S01 state owns a `Connection`, a selected output, and a `LayerSurface`. S01 acknowledges layer configure through SCTK before the handler callback and records a nonzero physical extent from logical size and integer output scale. S02 must not configure wgpu before that extent exists. wgpu 30.0.1 reports `CurrentSurfaceTexture::{Timeout, Occluded, Outdated, Lost, Validation}` rather than the older single-error API.

## Alternatives

1. Use `Surface<'static>` with a transmuted or leaked Wayland owner. Rejected: it silences the lifetime proof and violates the project safety rule.
2. Add a hidden window toolkit or a second Wayland connection. Rejected: it breaks SCTK ownership of the display/layer lifecycle and is out of scope.
3. Keep the renderer as `Renderer<'wayland>` borrowing the retained layer's `wl_surface`. Chosen: the compiler-visible lifetime ties the dependent surface to the protocol owner without a fabricated static lifetime.

## Decision

The S02 implementation uses these boundaries:

- `wayland_surface::run` retains the SCTK `Connection` and `LayerSurface` and performs the initial commit and configure roundtrip.
- `renderer::create_wayland_surface` is the only unsafe-code boundary. It obtains the display pointer from the pinned system Wayland backend and the window pointer from the retained `WlSurface` proxy, then calls `create_surface_unsafe` with a returned lifetime borrowed from both owners. The function documents pointer validity, same-thread use, and drop ordering.
- `Renderer<'wayland>` owns the wgpu `Instance`, `Surface<'wayland>`, adapter-created device/queue, capabilities-derived FIFO configuration, and solid-color pipeline. It is declared after the local `LayerSurface`, so it is dropped first; acquired surface frames are local to `render` and are therefore released before owner teardown.
- `WaylandClient` stores a clone of the `WlSurface` only for SCTK callback matching and stores lifecycle/pending-render state. The owning `LayerSurface` remains in the run function, not in a self-referential client/renderer pair.
- A `wl_surface.frame` callback is requested with SCTK's `FrameCallbackData` before each wgpu presentation. wgpu's `Queue::present` performs the presentation commit; Worldwall does not issue a duplicate manual commit. Resize/configure work waits for an outstanding callback to complete before submitting the next frame.
- FIFO is required from queried surface capabilities. The selected format prefers an advertised sRGB format and otherwise uses the first advertised format; alpha mode is selected from the advertised list. Zero extents are rejected before configure.
- Timeout and occlusion skip one frame. Outdated/lost surfaces receive one bounded surface recreation and retry; a second failure is fatal for this run. Validation is fatal. This is a bounded policy, not an infinite retry loop.

## Consequences and validation

This design permits the initial solid-color Vulkan frame on the assigned Hyprland host and keeps S01 output/layer teardown paths in the same event loop. The raw-handle proof is reviewable in `src/renderer.rs` and this record. The dependency lint changes from `unsafe_code = "forbid"` to `warn` so the single documented raw-handle function can use an explicitly allowed unsafe block; no other unsafe boundary is permitted by this design.

Pure tests cover zero-extent rejection, capability format selection, and bounded surface-error outcomes. The required review on 2026-09-21 confirmed the single unsafe boundary, borrowed Wayland lifetime, field drop order, and callback-before-present implementation against the pinned APIs. Locked format, clippy, test, and build checks pass, and the assigned-session trace reached Vulkan adapter selection, FIFO surface configuration, frame callback request, and presented frame. This ADR approval does not waive the remaining S02 live gates: manual pointer interaction, real output resize, injected surface failures, and compositor disconnect after presentation remain open in the evidence record.

## Supersession

No superseding decision exists.
