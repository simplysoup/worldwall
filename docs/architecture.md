# Architecture and runtime boundaries

Working specification for the [proposed contract](delivery-contract.md). API names in the archive are illustrative; S00/S02 must verify actual pinned library APIs.

## Ownership

```text
App / event dispatcher
├── Environment manager: clock, astronomy, optional async weather cache
├── Scene registry + pure selector: validated assets, selection snapshots
├── Renderer: instance → adapter → device/queue → shared scene resources
└── Output map: Wayland owners + dependent wgpu surface + configuration + pacing
```

SCTK owns the Wayland connection, event queue, globals, outputs, `wl_surface`, and layer role. wgpu owns device resources and presentation. Avoid a second window abstraction. CPU work includes events, uniform updates, command encoding/submission, and presentation; GPU animation does not eliminate it.

A single supported device is the baseline. Verify each surface's capabilities against it. Do not silently duplicate devices or textures to support an incompatible output; expose the unsupported output and keep compatible outputs running where possible.

## Output state machine

```text
Discovered → RoleCreated → AwaitingConfigure → Ready → Closing → Removed
                                  ↑             │
                                  └─ Reconfigure┘
```

1. Create the background layer, all anchors, keyboard interactivity none, explicit empty input region, and agreed exclusive zone (`-1` as in the outline, validated on Hyprland).
2. Commit the role's initial state with no content buffer; wait for configure. Verify whether the pinned SCTK callback already acknowledges it to avoid double-acknowledging.
3. Resolve configure size using protocol/toolkit rules. A zero component may mean client-chosen sizing; derive it from known output state or defer. Never configure wgpu with a zero extent or busy-wait for size.
4. Track logical extent separately from physical buffer extent, scale, and transform. S01 records integer-scale behavior; S09 records the supported integer-scale, assigned-host fractional-scale/viewport, and tested-transform matrix. When the protocols are unavailable, the client falls back to integer `wl_output` scaling.
5. Create/configure the wgpu surface with capabilities-derived format/alpha mode and FIFO. The display and surface must remain valid until the dependent wgpu surface and in-flight usage are safely released.
6. On resize, acknowledge the configure correctly, stop acquiring old-size frames, and reconfigure at a safe point. On removal, cancel callbacks/deadlines and drop dependent GPU presentation before Wayland owners. The connection outlives all surfaces.

Do not transmute or leak to manufacture `Surface<'static>`. A raw-handle creation boundary needs a written safety argument covering owner retention, pointer validity, threading, and drop order, with a reviewer. No implementation is prescribed until the pinned APIs are known.

## Scheduling

Each output has configure readiness, callback readiness, and a monotonic next-frame deadline. Render only when all applicable conditions are satisfied; initial/configure frames have an explicit bootstrap path. Request a frame callback **before** the commit/present whose completion it is intended to pace. Confirm what wgpu presentation commits instead of issuing speculative duplicate commits.

FIFO is presentation synchronization, not a target FPS limiter. Avoid callbacks triggering unbounded submissions, accumulated catch-up frames, and polling while callbacks are withheld. Suspend/no outputs means no render wakeups. Wayland events and IPC remain responsive while animation is throttled.

Use a bounded/wrapped animation phase or numerically stable timing representation to prevent long-running `f32` precision loss. Document whether animation excludes suspended time; the proposed default is a paused monotonic animation clock, unrelated to civil time.

## Scene replacement and resources

Decode/validate assets outside the frame hot path. Produce a complete candidate, upload its resources, then swap at an event-loop/frame boundary. Invalid candidates leave the last valid scene alive. Increment a generation ID so stale async work cannot overwrite a newer explicit command.

Share selected indices, palette, animation metadata, device, and queue. Keep per-output dimensions and uniform bindings separate: writing different sizes to the same uniform before multiple queued draws can cause all draws to see the last size. Cache compatible pipelines by presentation format if needed; one pipeline for unlike formats is not a requirement.

Only scene/content changes or GPU recovery re-upload full assets. Device recovery is an exception to the steady-state upload invariant and must be logged/measured separately.

## Environment and selection

The selector takes a validated registry and immutable environment snapshot; it performs no I/O. Weather work returns through a bounded completion channel. Cancellation/generation IDs and trigger coalescing prevent late fetches or duplicate resume signals from causing extra selections. Keep selection generations distinct from configuration/cache epochs and output-lifetime IDs: invalidating an old-location fetch or stale output callback is not a scene-selection event.

Refreshes have a documented finite deadline set by the S07 decision; when it expires, select using cached/unknown weather and continue. Timer results never call the selector. Local-time changes and astronomy do not affect the shader clock.

## Recovery boundaries

| Failure | Required behavior |
| --- | --- |
| Invalid scene/config | Reject candidate, retain previous valid state, actionable diagnostic |
| Surface outdated/lost | Bounded reconfigure/recreate using current valid extent |
| Surface timeout | Skip frame/back off without busy retry |
| Device loss / out of memory | Bounded tested recovery or clean diagnostic exit, no retry storm |
| No outputs | Idle, continue dispatch, wait for an output |
| Weather timeout/corrupt cache | Keep last valid cache or explicit unknown, never stop renderer |
| Missing power event service | Report degraded resume support; manual reload remains available |
| Compositor disconnect | Orderly teardown and documented exit code |

## Repository growth

Start with minimal `src/main.rs`, Wayland and renderer modules; introduce scene/environment/IPC modules when their stages need testable seams. The archive's full directory tree is a possible end state, not a scaffolding checklist. Commit a lockfile for the application after dependency versions are selected.
