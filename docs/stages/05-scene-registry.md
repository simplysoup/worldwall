# S05 — Scene registry and transactional selection (M2)

## Purpose

Select and replace multiple scenes without renderer-specific scene knowledge. Requirements: WW-03, WW-06, WW-11. Dependency: S04. State/owner: [roadmap](../roadmap.md).

## Entry conditions

S04 accepted; scene schema and effects frozen for v1. Use fake environment snapshots; real clock/astronomy/weather are later stages. Read [selection rules](../scene-format.md) and the contract event matrix.

## Deliverables

Manifest parser, validated registry/fallback, pure deterministic selector, explicit trigger API, safe asynchronous load/upload/swap boundary, and at least two lawful scene fixtures. No public IPC is required yet.

## Execution

1. Load scene manifests and validate version, ID uniqueness, paths, assets, tags, and designated fallback. Reject path escape and ambiguous registry state.
2. Implement weighted selection with explicit match semantics, injected RNG/seed, stable ties, empty-registry result, and logged rationale without sensitive location details.
3. Represent selection triggers explicitly: startup/resume/reload/manual ID/next. Ensure ordinary weather/output/config events cannot invoke this API accidentally.
4. Prepare a complete candidate outside rendering; validate/upload before atomically replacing active resources at a safe boundary. Use a generation token to discard stale completion work.
5. Implement manual ID and compatible `next` semantics via internal/test entrypoints. Do not write final socket code early.
6. Preserve the active scene when a candidate fails; use a valid offline fallback at startup. Skip re-upload when selected identity/content is unchanged.
7. Add fixture matrices for weights, wildcards, unknown values, ties, fallback, bad assets, competing requests, and registry reload while a scene is active.

## Acceptance criteria

- [ ] **S05-AC1:** Registry validation catches duplicates, invalid tags/versions/assets, missing fallback, and path escapes; errors leave a valid running scene intact.
- [ ] **S05-AC2:** Fixed snapshot/seed produces repeatable scoring and tie behavior; manual/next/fallback/empty-registry cases match the specified policy.
- [ ] **S05-AC3:** Only explicit triggers advance selection generation. Timer, output, and ordinary config events do not select.
- [ ] **S05-AC4:** Scene replacement is atomic; stale asynchronous completions cannot win over newer commands, failed candidates retain prior resources, and unchanged scenes avoid redundant uploads.
- [ ] **S05-AC5:** Multiple animated fixtures work without changing renderer code; reviewer accepts M2 using S04–S05 evidence.

## Failure and rollback

Keep the previous active scene until candidate success. A registry with no valid fallback is a startup/config error, not an unchecked panic. Use deterministic seeds and generation traces to reproduce races; never fix them by serially blocking the Wayland thread on asset/network I/O.

## Handoff

Publish manifest fixtures, selector API/ownership contract, trigger enumeration, generation semantics, and the M2 demo. S06 should supply environment snapshots without changing selection ownership.
