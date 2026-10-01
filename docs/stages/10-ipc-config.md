# S10 — Local controls and transactional configuration

## Purpose

Expose safe, bounded user controls and understandable operational state. Requirements: WW-06, WW-10, WW-11. Dependency: S09. State/owner: [roadmap](../roadmap.md).

## Entry conditions

S09 accepted. Freeze a versioned command/request/response schema, request limits/timeouts, and config-reload policy consistent with the [contract event matrix](../delivery-contract.md). Do not conflate config reload with scene reselection.

## Deliverables

`worldwallctl`, same-user Unix socket, protocol tests, atomic schema-v1/v2/v3/v4 config validation/application (including explicit location/timezone, optional weather, default-off configured adaptive selection, and the global animation-speed multiplier), the CR-0010 process-local `adaptive status|on|off` command, structured rate-limited diagnostics, and user-facing command documentation.

## Execution

1. Bind under a validated user-owned `XDG_RUNTIME_DIR` (conventional path `/run/user/$UID/worldwall.sock`), restrictive permissions, and peer-credential policy. Reject unsafe directories/symlinks and foreign/live socket ownership; do not blindly unlink a path.
2. Define protocol version, framing, maximum request/response size, command timeout, error codes, and success payloads. Bound clients/queues and handle partial/malformed requests.
3. Route commands to the event-loop owner; never mutate GPU/Wayland resources directly from client I/O threads.
4. Implement `status` (outputs, scene, weather freshness, FPS, backend, degraded state), `weather` (cache only), `reload` (bounded refresh + selection), `scene <id>`, `next`, and `reload-config` (transactional config, no reselection).
5. Validate complete candidate config and resource consequences before committing it. Preserve schema v1-v3; schema v3 adds `[weather].adaptive_scenes = false` by default and accepts true only with enabled Open-Meteo, explicit location/timezone, and a validated adaptive registry. Schema v4 adds `animation_speed_multiplier` in `0.0..=1.0`, defaulting to `0.5` when absent in every supported schema. CR-0010 adds a runtime-only adaptive override: prepare the source set off the Wayland owner, commit it with the effective mode atomically, and leave the config file authoritative on reload/restart.
6. Keep the active scene resident if registry/location/timezone/provider paths change; reject config lacking a viable future fallback. Apply weather/location changes without leaking credentials or reusing a cache from a changed config generation; retire the old worker before starting a newly enabled provider.
7. Test socket ownership, concurrent clients, stalled connections, stale sockets, invalid IDs/config, command ordering, disconnect, clean shutdown, and double startup.

## Acceptance criteria

- [ ] **S10-AC1:** All six command forms have documented stable semantics, output/error examples, and end-to-end tests against the running service.
- [ ] **S10-AC2:** Socket ownership/permissions/peer policy, framing/size/time limits, concurrent clients, and stale-path handling are tested; clients cannot block rendering or mutate state unsafely.
- [ ] **S10-AC3:** `reload` selects once, `reload-config` never selects, `weather` never fetches, and failed commands leave active state unchanged. A successful config reload invalidates queued adaptive candidates; late async completions respect command generation.
- [ ] **S10-AC4:** Invalid config is rejected atomically; valid changes reconfigure only intended services and preserve current scene/resources until an explicit selection.
- [ ] **S10-AC5:** Status/logging exposes degraded/freshness/backend/output state without secrets or exact location, and diagnostics are rate-limited under repeated failures.
- [ ] **S10-AC6 (CR-0005):** Schema-v1 compatibility and schema-v2 authority/validation are tested; valid location/timezone/provider reloads apply atomically without reselection; invalid replacements preserve the active config, scene, and provider; weather stays disabled unless explicitly enabled.
- [ ] **S10-AC7 (CR-0006):** Schema-v3 compatibility/validation, adaptive registry requirements, privacy-safe family/band persistence, config-reload invalidation, and default-off behavior pass. No config reload or late candidate changes family outside an explicit full-selection trigger.
- [ ] **S10-AC8 (CR-0006):** IPC/config and scene-package loading leave the Wayland event thread responsive; asynchronous candidate completion preserves established command and transactional failure semantics.
- [ ] **S10-AC9 (CR-0007):** Schema-v4 multiplier parsing/range/default compatibility and transactional reload are tested; a successful rate change preserves scene identity/generation and animation phase, while an invalid candidate leaves config, rate, and resident scene unchanged.
- [ ] **S10-AC10 (CR-0010 supplemental):** `worldwallctl adaptive status|on|off` has bounded protocol/CLI and running-service coverage. Status reports effective mode; on/off are idempotent and process-local. Enabling rejects missing Open-Meteo, explicit location/timezone, or complete adaptive registry. Both transitions preserve resident scene/generation, never fetch or select, prepare/commit validated sources atomically, and roll back on failure. `reload-config` restores the configured file value. Offline tests and assigned-session IPC evidence are reviewed separately from historical S10 acceptance.

## Failure and rollback

Reject ambiguous or incompatible protocol/config versions with clear errors. Never unlink another user's object or a live service socket to force startup. Retain the old config until the new one is fully valid; if partial application cannot be rolled back, redesign the transaction rather than documenting partial success as normal.

## Handoff

Provide protocol/config references, permission tests, CLI examples, generation ordering tests, and diagnostics catalogue. CR-0005 adds IANA/location/weather reload tests and privacy/cache-scope evidence. S11 uses real commands for acceptance and release runbooks.
