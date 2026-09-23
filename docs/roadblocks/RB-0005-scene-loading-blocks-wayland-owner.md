# RB-0005 — Registry decoding still runs on the Wayland event owner

- State: resolved (bounded async paths and deterministic offline tests; acceptance remains open)
- Opened / last updated: 2026-09-23
- Stage / related criteria: CR-0006 supplemental S05-AC4/AC7 and S10-AC8 async scene/config gates; assigned-session acceptance is still open
- Owner / escalation target: Pi coding agent; maintainer review by `simplysoup` before any IPC response-semantics change
- Kind / impact: technical/architecture; high
- Next check: maintainer review of the implementation; any assigned-session evidence remains a separately authorized follow-up

## Expected versus observed

**Expected:** scene decoding/registry candidate work stays off the Wayland event thread, then uses the existing request-generation and transactional renderer-swap boundary.

**Clarification:** This is not an inability to implement selection triggers. Startup/resume, `reload`, `scene <id>`, and `next` can choose a scene; a successful scene transaction swaps the scene and its palette/resources together. The active scene's time-of-day/solar timeline can also change its palette without scene reselection, while a fresh adaptive weather result is limited to a precipitation variant in the active family. `reload-config` remains non-selecting. RB-0005 blocks claiming responsive off-thread loading and acceptance until the decode/config validation path is moved behind a bounded worker, not the trigger semantics themselves.

**Historical observation (before this increment):** adaptive weather-variant and resume work used `SceneLoadWorker`, but `handle_ipc_requests` synchronously called `SceneManifest::load_scene()` for adaptive `reload` and manual `scene`/`next`. It also loaded replacement config on the event-loop owner; `AppConfig::load` validated registry assets through `SceneRegistry::load`. The confirmed issue was a synchronous call path; no visible stall duration was measured.

**Current source review:** startup config validation and startup scene selection/decode now run on short-lived loader threads before event-loop dispatch. Runtime config parsing plus complete registry validation return a prepared config/registry from a bounded worker. Explicit IPC scene loads and adaptive background loads use separate one-job workers; a calloop channel wakes the owner even with no frame timer. The owner processes IPC requests FIFO, handles config/selection completions, and performs the existing transactional apply/upload/swap. Scene/config jobs carry the validated data needed for commit; no Wayland or GPU handles leave the owner. No Worldwall runtime was started to measure a visible stall; this record resolves the source-level technical blocker and deterministic harness requirement only.

## Diagnosis and attempts

| Hypothesis | Experiment | Result / evidence | Conclusion |
| --- | --- | --- | --- |
| All adaptive candidate decoding is already off-thread. | Trace the adaptive `reload` IPC branch and successful config-reload path through the current source. | Adaptive reload directly calls `job.manifest.load_scene()`; config loading eagerly validates packages on the event-loop owner. | Rejected. Weather/resume are async; reload/config paths are not. |
| Existing explicit `scene`/`next` commands are safe because they are not weather-triggered. | Trace the `begin_explicit_request` caller and candidate load. | `manifest.load_scene()` runs synchronously in `handle_ipc_requests`. | Rejected; the issue also affects explicit scene commands. |

No runtime, network, or hardware experiment was performed.

## Initial workaround and escalation (historical)

Before implementation, preserve IPC final result semantics and the all-or-nothing config transaction. A pending/accepted response was not adopted because it would change the protocol. The implementation preserves the final success/error response and two-second timeout; it does not claim work completing beyond that timeout will be reported before the client gives up.

## Resolution evidence

- Runtime `reload`, `scene <id>`, `next`, adaptive `reload`, and `reload-config` no longer decode assets or validate registries on the Wayland dispatch owner. Config workers return a complete `PreparedAppConfig`, including its already validated registry, so config commit does not reload package assets on the owner.
- Separate bounded single-slot workers isolate explicit IPC loads from adaptive weather/resume loads. Scene and config completion channels wake calloop without relying on an animation or weather timer. The owner serializes IPC requests FIFO while continuing Wayland, frame, power, and weather-cache handling.
- Generation invalidation and existing transaction tests cover superseded/failing candidates and config rollback; fake blocked-loader tests prove a calloop timer dispatches while config or scene validation is blocked, and queue-capacity/error tests cover both worker families.
- `cargo fmt --all --check`, `cargo clippy --offline --all-targets --all-features -- -D warnings`, and `cargo test --offline --all-targets --all-features` passed after implementation (recorded in the linked update). No Wayland runtime stall measurement, live scene swap, adaptive activation, or assigned-session S05-AC7 result is claimed. Historical S05/S10 acceptance remains unchanged; any live/assigned-session evidence is still required by its separate gates.

## Related records

- [CR-0006](../changes/CR-0006-weather-adaptive-scene-selection.md)
- [CR-0006 implementation update](../updates/2026-09-23-cr0006-weather-adaptive-scenes.md)
- [RB-0005 bounded-loading update](../updates/2026-09-23-rb0005-bounded-loading.md)
