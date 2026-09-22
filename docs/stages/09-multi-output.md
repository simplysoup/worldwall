# S09 — Multi-output lifecycle and shared resources

## Purpose

Extend the proven engine across outputs without duplicating scene assets or corrupting per-output state. Requirements: WW-01, WW-02, WW-05, WW-09, WW-11. Dependency: S08. State/owner: [roadmap](../roadmap.md).

## Entry conditions

S08 accepted. Approve the supported output scale/transform/device matrix and secure a multi-output live test environment. Single-device support does not promise arbitrary multi-GPU topology.

## Deliverables

Output map with independent layer/presentation/configuration/pacing state, shared scene textures/device/queue, per-format pipelines where necessary, and no-output idle/hotplug recovery.

## Execution

1. Move proven output-local owners into keyed records; reject stale callbacks/events after removal using stable identities/generations.
2. Query each new surface's device compatibility and capabilities. Reuse compatible pipelines; create format-specific variants only when needed.
3. Give each output independent physical dimensions, scale/transform mapping, uniform bindings/offsets, callback state, and FPS deadline.
4. Render the same shared scene/animation time on all outputs. Test that queued draws never all read the last-written output dimensions.
5. Handle add/remove/resize/scale/rotation and reconnect while rendering; drop GPU presentation before its Wayland owners without tearing down shared resources still in use.
6. With zero outputs, cancel frame wakeups and idle while dispatching events (and IPC once added in S10); restore on output arrival. Hotplug must not reselect the scene.
7. Measure allocation/upload counts and per-output submission timestamps. Document unsupported adapter/output combinations without silently creating a second device.

## Acceptance criteria

- [ ] **S09-AC1:** Live mixed-size/refresh/scale outputs display the same selected scene with correct independent cover mapping; supported transforms/fractional scale have explicit evidence or an approved narrowed matrix.
- [ ] **S09-AC2:** Allocation/upload evidence confirms one shared selected-scene texture set and device/queue; separate output uniforms/pipelines are correctly scoped.
- [ ] **S09-AC3:** Hotplug/unplug during frames, resize, no-output idle, reconnect, and topology change around sleep work without use-after-free, panic, stale callbacks, or busy wakeups.
- [ ] **S09-AC4:** Each output observes its FPS ceiling/readiness; one slow/hidden output does not busy-spin or corrupt another's state.
- [ ] **S09-AC5:** Hotplug never advances selection generation. Incompatible outputs produce an actionable bounded degraded state while compatible outputs remain usable where possible.

**Acceptance boundary:** [CR-0004](../changes/CR-0004-s09-incompatible-output-evidence-deferral.md) approves S09 acceptance using the bounded unit/source capability-failure evidence and compatible-output live evidence. A physical incompatible-capability output remains explicit future work and `not-run`; no universal hardware-compatibility claim is made.

## Failure and rollback

Do not fix shared-uniform corruption by duplicating the full renderer or scene textures. Isolate per-output state and retain a one-output reproduction. Unsupported hardware is documented as unsupported, not passed; required but unavailable test hardware creates a roadblock.

## Handoff

Provide output capability matrix, ownership/drop tests, mixed-output evidence, allocation counts, and known limitations. S10 exposes these states through IPC without mutating them from arbitrary socket threads.
