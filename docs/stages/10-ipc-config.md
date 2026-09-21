# S10 — Local controls and transactional configuration

## Purpose

Expose safe, bounded user controls and understandable operational state. Requirements: WW-06, WW-10, WW-11. Dependency: S09. State/owner: [roadmap](../roadmap.md).

## Entry conditions

S09 accepted. Freeze a versioned command/request/response schema, request limits/timeouts, and config-reload policy consistent with the [contract event matrix](../delivery-contract.md). Do not conflate config reload with scene reselection.

## Deliverables

`worldwallctl`, same-user Unix socket, protocol tests, atomic config validation/application, structured rate-limited diagnostics, and user-facing command documentation.

## Execution

1. Bind under a validated user-owned `XDG_RUNTIME_DIR` (conventional path `/run/user/$UID/worldwall.sock`), restrictive permissions, and peer-credential policy. Reject unsafe directories/symlinks and foreign/live socket ownership; do not blindly unlink a path.
2. Define protocol version, framing, maximum request/response size, command timeout, error codes, and success payloads. Bound clients/queues and handle partial/malformed requests.
3. Route commands to the event-loop owner; never mutate GPU/Wayland resources directly from client I/O threads.
4. Implement `status` (outputs, scene, weather freshness, FPS, backend, degraded state), `weather` (cache only), `reload` (bounded refresh + selection), `scene <id>`, `next`, and `reload-config` (transactional config, no reselection).
5. Validate complete candidate config and resource consequences before committing it. Keep the active scene resident if registry paths change; reject config lacking a viable future fallback.
6. Apply weather/location changes without leaking credentials or letting old-location responses populate the new cache. Rearm services under the new generation safely.
7. Test socket ownership, concurrent clients, stalled connections, stale sockets, invalid IDs/config, command ordering, disconnect, clean shutdown, and double startup.

## Acceptance criteria

- [ ] **S10-AC1:** All six command forms have documented stable semantics, output/error examples, and end-to-end tests against the running service.
- [ ] **S10-AC2:** Socket ownership/permissions/peer policy, framing/size/time limits, concurrent clients, and stale-path handling are tested; clients cannot block rendering or mutate state unsafely.
- [ ] **S10-AC3:** `reload` selects once, `reload-config` never selects, `weather` never fetches, and failed commands leave active state unchanged. Late async completions respect command generation.
- [ ] **S10-AC4:** Invalid config is rejected atomically; valid changes reconfigure only intended services and preserve current scene/resources until an explicit selection.
- [ ] **S10-AC5:** Status/logging exposes degraded/freshness/backend/output state without secrets or exact location, and diagnostics are rate-limited under repeated failures.

## Failure and rollback

Reject ambiguous or incompatible protocol/config versions with clear errors. Never unlink another user's object or a live service socket to force startup. Retain the old config until the new one is fully valid; if partial application cannot be rolled back, redesign the transaction rather than documenting partial success as normal.

## Handoff

Provide protocol/config references, permission tests, CLI examples, generation ordering tests, and diagnostics catalogue. S11 uses real commands for acceptance and release runbooks.
