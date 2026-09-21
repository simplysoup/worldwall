---
name: worldwall-renderer-review
description: Review Worldwall Wayland and wgpu integration, raw-handle lifetimes, indexed shaders, frame pacing, scene uploads, or multi-output changes. Use before merging renderer changes or diagnosing presentation, color, power, and teardown regressions.
---

# Review renderer invariants

Read [architecture](../../../docs/architecture.md), [scene format](../../../docs/scene-format.md), [validation](../../../docs/validation.md), and only affected S01–S04/S09 stage gates. The archived snippets are not pinned API documentation.

## Review checklist

1. **Owners:** display and `wl_surface` outlive the dependent GPU surface and in-flight use; narrow unsafe boundary has explicit validity/thread/drop proof. No transmute/leak to invent `'static`.
2. **Protocol:** Background role, anchors, empty input region, configure/ack exactly once under actual toolkit semantics, valid nonzero physical extent, scale/transform mapping, safe close/remove.
3. **Presentation:** same Wayland surface, queried capabilities, supported adapter/format/alpha, FIFO plus target limiter. Request frame callback before its intended commit/present; inspect actual wgpu commit behavior.
4. **Scheduling:** bootstrap path; per-output callback readiness AND monotonic deadline; no catch-up burst, spin, or millisecond polling; zero outputs/sleep stop render wakeups.
5. **Resources:** raw `R8Uint` indices, integer loads, correct color path, opaque palette, validated metadata, no steady-state full uploads/CPU recoloring, atomic candidate replacement.
6. **Outputs:** shared scene/device/queue, independent uniform bindings/dimensions/lifetimes/pacing, per-format pipeline compatibility, no last-output-size overwrite.
7. **Failures:** bounded surface timeout/lost/outdated and fatal-device behavior; stale callbacks/generations rejected; previous valid scene retained on failed load.
8. **Evidence:** reference vectors/readback and fake-clock tests plus actual compositor resize/input/hotplug/shutdown evidence. Capture benchmark counters before claiming efficiency.

## Result

Return findings by severity with file/line or proposed-interface pointers, affected gate IDs, reproduction/evidence, and required next action. Missing hardware checks remain open. Record substantial inability to proceed as a roadblock; changed requirements require a roadmap proposal. A clean review of a document is not runtime verification.
