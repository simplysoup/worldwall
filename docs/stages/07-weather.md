# S07 — Optional asynchronous weather and cache

## Purpose

Add low-frequency weather inputs for future selections without disrupting rendering. Requirements: WW-06, WW-07, WW-11. Dependency: S06. State/owner: [roadmap](../roadmap.md).

## Entry conditions

S06 accepted. ADR-0006 approves provider, terms, normalized mapping, request/refresh deadlines, stale-cache rules, limits, and backoff. CR-0005 moves location/provider consent into schema-v2 config. CR-0006 adds a schema-v3 default-off adaptive policy; no request occurs unless `open-meteo` is explicitly selected with a complete configured location.

## Deliverables

Provider abstraction with fake client, normalized weather type, bounded async requests, versioned atomic cache with configuration-generation scoping, normal approximately 15-minute poll scheduling, freshness diagnostics, and offline-first behavior.

## Execution

1. Store normalized condition, finite optional temperature in Celsius, Boolean precipitation, UTC observed/fetched times, source, and freshness. If retaining precipitation amount, name the unit explicitly; do not overload the Boolean field.
2. Represent disabled/unknown/fresh/stale/error states distinctly. Configure storage under the user's XDG cache location with restrictive access; redact coordinates/credentials in logs.
3. Schedule one request at a time. Enforce timeout, bounded retries/backoff/rate limits, cancellation, and no per-frame work. Test 15-minute cadence with fake time instead of waiting in real tests.
4. Read/write cache using version validation and atomic replacement; reject corrupt/unbounded/future-invalid data without overwriting the last valid memory state.
5. For full-selection triggers, use the approved finite deadline and available fresh/stale/unknown cache. Without adaptive opt-in, a late successful request updates cache only. With valid opt-in, a late fresh known result may update only the precipitation variant inside the active family; it cannot trigger a family choice.
6. Exercise offline startup, network timeout, bad provider payload, authorization/rate errors, provider/location config changes and cache invalidation, and stale/future timestamps with fixtures.
7. Perform a limited configured live-provider check only if authorized and terms/rate limits allow. Unit/fake tests remain the main coverage.

## Acceptance criteria

- [ ] **S07-AC1:** Disabled/unconfigured weather makes zero requests; enabled normal cadence is no faster than the approved approximately 15-minute policy, with bounded error retries and at most one in-flight request.
- [ ] **S07-AC2:** Provider normalization, timestamp/freshness rules, versioned cache, corrupt-file handling, and atomic writes pass deterministic tests.
- [ ] **S07-AC3:** Slow/offline weather never blocks Wayland dispatch or animation; explicit refresh completes selection within its finite deadline using a valid fallback snapshot.
- [ ] **S07-AC4:** With adaptive selection absent/disabled, weather poll/completion (including late completion) changes cache only. With valid CR-0006 opt-in, only a fresh known result may request a same-family precipitation variant; stale, error, unknown, or failed candidates retain the valid scene, and weather never changes family.
- [ ] **S07-AC5:** Location/credentials stay out of ordinary logs and status; data-source/terms/configuration/privacy instructions are documented and reviewed.
- [ ] **S07-AC6 (CR-0005):** Configured weather defaults off and makes zero requests; enabled Open-Meteo reuses the explicit configured location. Config reload provider/location changes invalidate the prior scoped cache, cancel/retire the old worker before a new request, and never reselect a scene.
- [ ] **S07-AC7 (CR-0006):** Schema-v3 opt-in and the complete adaptive path pass deterministic freshness/fallback/failure/privacy tests; a successful config reload invalidates queued candidates, weather cannot switch families, and assigned-session evidence covers same-family swap behavior.

## Failure and rollback

Retain last valid weather with visible age/error; use unknown when none exists. Do not auto-enable another provider, IP geolocation, or faster polling after a failure. Provider selection/deadline uncertainty blocks this stage until decided, not the already-working renderer.

## Handoff

Provide provider decision, consent/config shape, mock responses, cache schema/scope, timeout/backoff values, request/selection traces, and privacy review. CR-0005 retains no raw coordinates in the cache or ordinary diagnostics. S08 reuses the bounded refresh orchestration instead of adding a second weather path.
