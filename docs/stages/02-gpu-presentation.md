# S02 — Safe wgpu presentation

## Purpose

Present a static solid color/fullscreen triangle on S01's exact Wayland surface. Requirements: WW-02, WW-11. Dependency: S01. State/owner: [roadmap](../roadmap.md). This is not yet the static indexed-image MVP.

## Entry conditions

S01 accepted; live Vulkan host available; pinned raw-handle APIs inspected. Read architecture ownership and scheduling sections. Create an ADR for the concrete safe/unsafe ownership design before merging it.

## Deliverables

Shared instance/adapter/device/queue, one dependent presentation surface, capabilities-derived configuration, minimal WGSL pipeline, and safe teardown/reconfigure handling.

## Execution

1. Acquire raw display/surface handles from retained Wayland owners using the pinned backend. Prove validity and thread constraints for the entire GPU-surface lifetime.
2. Create the wgpu surface on the same display/surface; select a compatible Vulkan adapter and query supported formats/alpha modes. Avoid hidden windows and unconditional format assumptions.
3. Wait for valid configure/ack and nonzero physical extent before surface configuration. Configure FIFO and documented frame latency where supported by the pinned API.
4. Render an opaque solid color/fullscreen triangle. Request compositor frame notification before the associated commit/present when using callbacks; trace actual wgpu commit behavior.
5. Implement a bootstrap frame plus event-driven redraw/reconfigure without a busy loop. A static scene need not continuously redraw.
6. Match actual surface acquisition error variants to bounded timeout/lost/outdated/device-fatal behavior. Test injected errors separately from real resize.
7. On output close/disconnect/shutdown, release acquired frames and GPU surface usage before destroying layer/surface/display owners. Review every unsafe block with its safety argument.

## Acceptance criteria

- [ ] **S02-AC1:** Live color/triangle appears behind ordinary apps; desktop input passes through and no focus is captured. Record manual procedure, not screenshot alone.
- [ ] **S02-AC2:** Reviewer approves raw-handle lifetime/drop-order proof; startup/resize/shutdown stress shows no invalid handle access or validation errors.
- [ ] **S02-AC3:** Rendering begins only after valid configure/ack, uses supported surface configuration, and responds correctly to size changes without zero-size configuration.
- [ ] **S02-AC4:** Callback request/commit ordering and initial-frame behavior are demonstrated on pinned APIs; idle/redraw path does not spin.
- [ ] **S02-AC5:** Timeout/lost/outdated/fatal cases have bounded, asserted outcomes; layer close and compositor disconnect exit cleanly with useful diagnostics.

## Failure and rollback

Keep S01's layer-only path as a diagnostic baseline. If adapter/surface compatibility fails, capture capabilities/versions and a minimal reproducer. Do not loosen lifetimes, leak owners, or add a second window backend as a workaround without a reviewed change. Missing hardware validation leaves this stage unaccepted.

## Handoff

Provide the safety ADR, actual API matrix, presentation trace, supported format path, error policy, and evidence. S03 should only add resources/shader lookup, not rewrite the ownership boundary.
