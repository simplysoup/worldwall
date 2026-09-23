# S05 — Scene registry and transactional selection (M2)

## Purpose

Select and replace multiple scenes without renderer-specific scene knowledge. Requirements: WW-03, WW-06, WW-11. Dependency: S04. State/owner: [roadmap](../roadmap.md).

## Entry conditions

S04 accepted; ordinary package-v1 behavior remains compatible. CR-0006 adds opt-in package-v2 family/band metadata. Use fake environment snapshots; read the [selection rules](../scene-format.md) and contract event matrix.

## Deliverables

Manifest parser, validated registry/fallback, pure deterministic selector, explicit trigger API, safe asynchronous load/upload/swap boundary, and at least two lawful scene fixtures. No public IPC is required yet.

## Execution

1. Load scene manifests and validate version, ID uniqueness, paths, assets, tags, and designated fallback. Reject path escape and ambiguous registry state.
2. Implement weighted selection with explicit match semantics, injected RNG/seed, stable ties, empty-registry result, and logged rationale without sensitive location details.
3. Represent full-selection triggers explicitly: startup/resume/reload/manual ID/next. With valid CR-0006 opt-in, fresh weather may request only a precipitation variant in the active family; it must not invoke full family selection. Clock/output/config events never select.
4. Decode/validate scene candidates off the Wayland event thread, then upload and atomically replace active resources at the existing safe boundary. Use request generations to discard stale work after newer explicit selection or successful config reload.
5. Implement manual ID and compatible `next` semantics via internal/test entrypoints. Do not write final socket code early.
6. Preserve the active scene when a candidate fails; use a valid offline fallback at startup. Skip re-upload when selected identity/content is unchanged.
7. Add fixture matrices for weights, wildcards, unknown values, ties, fallback, bad assets, competing requests, and registry reload while a scene is active.

## Acceptance criteria

- [ ] **S05-AC1:** Registry validation catches duplicates, invalid tags/versions/assets, missing fallback, and path escapes; errors leave a valid running scene intact.
- [ ] **S05-AC2:** Fixed snapshot/seed produces repeatable scoring and tie behavior; manual/next/fallback/empty-registry cases match the specified policy.
- [ ] **S05-AC3:** Startup/resume/reload/manual ID/next are the only full-selection triggers. With valid CR-0006 opt-in, an accepted fresh weather result may request only a same-family precipitation variant; it cannot change family. Clock, output, and config events do not select.
- [ ] **S05-AC4:** Scene decoding is off the Wayland event thread; replacement is atomic; stale async completions cannot win over newer commands or successful config reload; failed candidates retain prior resources; unchanged scenes avoid redundant uploads.
- [ ] **S05-AC5:** Multiple animated fixtures work without changing renderer code; reviewer accepts M2 using S04–S05 evidence.
- [ ] **S05-AC6 (CR-0006):** Opt-in package-v2 metadata validates explicit families/bands, one precipitation tag per variant, all five approved bands, per-family clear fallback, and clear V29 offline fallback; temperature/hysteresis and same-family weather fallback tests pass.
- [ ] **S05-AC7 (CR-0006):** Assigned-session validation confirms an adaptive variant swap uses the shared transactional renderer boundary without a family switch, Wayland-thread decode, or per-frame scene upload.

## Failure and rollback

Keep the previous active scene until candidate success. A registry with no valid fallback is a startup/config error, not an unchecked panic. Use deterministic seeds and generation traces to reproduce races; never fix them by serially blocking the Wayland thread on asset/network I/O.

## Handoff

Publish manifest fixtures, selector API/ownership contract, trigger enumeration, generation semantics, and the M2 demo. CR-0003's catalog inventory/package-validation follow-up is recorded in the [CR-0003 evidence](../evidence/2026-09-22-cr003-scene-variants.md). S06 should supply environment snapshots without changing selection ownership.
