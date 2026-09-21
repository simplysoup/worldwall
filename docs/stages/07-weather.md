# S07 — Optional asynchronous weather and cache

## Purpose

Add low-frequency weather inputs for future selections without disrupting rendering. Requirements: WW-06, WW-07, WW-11. Dependency: S06. State/owner: [roadmap](../roadmap.md).

## Entry conditions

S06 accepted. Approve one provider, terms, explicit location/config consent, normalized mapping, request/refresh deadlines, stale-cache rules, cache limits, and backoff. No real requests until the user enables/configures the feature.

## Deliverables

Provider abstraction with fake client, normalized weather type, bounded async requests, versioned atomic cache, normal approximately 15-minute poll scheduling, freshness diagnostics, and offline-first behavior.

## Execution

1. Store normalized condition, finite optional temperature in Celsius, Boolean precipitation, UTC observed/fetched times, source, and freshness. If retaining precipitation amount, name the unit explicitly; do not overload the Boolean field.
2. Represent disabled/unknown/fresh/stale/error states distinctly. Configure storage under the user's XDG cache location with restrictive access; redact coordinates/credentials in logs.
3. Schedule one request at a time. Enforce timeout, bounded retries/backoff/rate limits, cancellation, and no per-frame work. Test 15-minute cadence with fake time instead of waiting in real tests.
4. Read/write cache using version validation and atomic replacement; reject corrupt/unbounded/future-invalid data without overwriting the last valid memory state.
5. For startup/resume/manual refresh, select within the approved finite deadline using fresh, stale, or unknown data. A late successful request updates cache only, never triggers a second selection.
6. Exercise offline startup, network timeout, bad provider payload, authorization/rate errors, location change, and stale/future timestamps with fixtures.
7. Perform a limited configured live-provider check only if authorized and terms/rate limits allow. Unit/fake tests remain the main coverage.

## Acceptance criteria

- [ ] **S07-AC1:** Disabled/unconfigured weather makes zero requests; enabled normal cadence is no faster than the approved approximately 15-minute policy, with bounded error retries and at most one in-flight request.
- [ ] **S07-AC2:** Provider normalization, timestamp/freshness rules, versioned cache, corrupt-file handling, and atomic writes pass deterministic tests.
- [ ] **S07-AC3:** Slow/offline weather never blocks Wayland dispatch or animation; explicit refresh completes selection within its finite deadline using a valid fallback snapshot.
- [ ] **S07-AC4:** Weather poll/completion, including a late refresh result, changes cache only. Selection-generation counters and scene identity remain unchanged until an explicit trigger.
- [ ] **S07-AC5:** Location/credentials stay out of ordinary logs and status; data-source/terms/configuration/privacy instructions are documented and reviewed.

## Failure and rollback

Retain last valid weather with visible age/error; use unknown when none exists. Do not auto-enable another provider, IP geolocation, or faster polling after a failure. Provider selection/deadline uncertainty blocks this stage until decided, not the already-working renderer.

## Handoff

Provide provider decision, consent/config shape, mock responses, cache schema, timeout/backoff values, request/selection traces, and privacy review. S08 reuses the bounded refresh orchestration instead of adding a second weather path.
