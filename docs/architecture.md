# Architecture and runtime boundaries

Working specification for the approved [delivery contract v0.8](delivery-contract.md). API names in the archive are illustrative; S00/S02 must verify actual pinned library APIs.

## Ownership

```text
App / event dispatcher
├── Environment manager: UTC clock, explicit IANA/local date, astronomy, optional async weather cache
├── Scene registry + pure selector: validated assets, selection snapshots
├── Renderer: instance → adapter → device/queue → shared scene resources and palette sample
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

Use a bounded/wrapped animation phase or numerically stable timing representation to prevent long-running `f32` precision loss. Animation uses a paused monotonic clock, unrelated to civil time. The schema-v4 `animation_speed_multiplier` defaults to 0.5 and rate-integrates all palette effects globally; changing it preserves phase, and the bounded shader phase remains the only animation timing input.

## Scene replacement and resources

Startup config/package validation, runtime config-registry validation, explicit IPC scene loads, and adaptive weather/resume candidates run on bounded loader workers outside the Wayland dispatch thread. Runtime IPC is serialized in FIFO order while those workers run; completion channels wake calloop independently of frame timers. Produce a complete CPU candidate, upload its resources on the owner, then swap at the existing event-loop transaction boundary. Invalid candidates leave the last valid scene alive. Internal request generations invalidate stale work after a newer full selection or successful config reload without counting config reload itself as a selection. Weather candidates may replace only a precipitation variant in the active family; they cannot update the family/band state. With the approved S12 pool sidecar, the selector also gates every automatic candidate by enabled-pool, scene-enable, and time/weather/temperature rules; manual `scene <id>` is an explicit override. The curator/library remains outside the adaptive registry, and successful config reload applies its rules without selecting. See the [IPC reference](ipc.md) for queue/response behavior.

Share selected indices, palette, animation metadata, device, and queue. Keep per-output dimensions and uniform bindings separate: writing different sizes to the same uniform before multiple queued draws can cause all draws to see the last size. Cache compatible pipelines by presentation format if needed; one pipeline for unlike formats is not a requirement.

Only scene/content changes or GPU recovery re-upload full assets. Device recovery is an exception to the steady-state upload invariant and must be logged/measured separately.

## Environment and selection

The selector takes a validated registry and immutable environment snapshot; it performs no I/O. Weather work returns through a bounded completion channel. Cancellation/generation IDs and trigger coalescing prevent late fetches or duplicate resume signals from causing extra selections. Keep selection generations distinct from configuration/cache epochs and output-lifetime IDs: invalidating an old-location fetch or stale output callback is not a scene-selection event.

Schema-v2 config accepts an explicit location and IANA timezone; schema v3 adds the default-off adaptive-scene opt-in inside `[weather]`; schema v4 adds the global `animation_speed_multiplier`; approved S12 schema v5 will add an optional `scene_pools_file` sidecar reference. No host timezone or location is inferred. The environment layer derives local civil time and NOAA events, caches previous/current/next local-date event sets, and resolves solar keyframes to a lower atlas row, upper atlas row, and fraction. Missing/polar/duplicate/unbracketed solar data selects the base palette for that timeline. The shader receives only that bounded sample, not coordinates, timezone, weather, or selection state. Schema-v1 civil timelines retain their existing local-seconds input.

Full-selection triggers have a documented finite environment deadline; when it expires, select using cached/unknown weather and continue. A later accepted fresh weather result may enter the CR-0006 selector path only to choose a same-family precipitation variant; it never changes family. With the S12 sidecar, that and every other automatic selection must choose an eligible candidate; manual `scene <id>` bypasses pool/availability filters. Config reload invalidates queued adaptive candidates, applies pool rules, and never reselects. Clock/solar changes update only resident palette inputs and do not affect the monotonic shader clock or selection.

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
