# S08 — Suspend/resume orchestration (M3)

## Purpose

Refresh the environment once on real resume while safely pausing/restarting rendering. Requirements: WW-06, WW-08, WW-11. Dependency: S07. State/owner: [roadmap](../roadmap.md).

## Entry conditions

S07 accepted. Approve/verify a power-event adapter on the target host (for example logind PrepareForSleep) and the paused animation-clock policy. Do not assume a signal API or permission from the conceptual outline.

## Deliverables

Testable sleep-state machine, real power-event subscription, duplicate/coalesced trigger handling, graceful missing-service behavior, and live suspend/resume evidence.

## Execution

1. Isolate power/session signals behind an adapter so unit tests can emit entering-sleep/resumed/disconnected events deterministically.
2. On sleep, stop render deadlines/submission and unnecessary refresh work without destroying valid scene ownership prematurely.
3. On resume, re-evaluate output/surface readiness, refresh clock/astronomy and bounded stale weather, then select once with an immutable snapshot.
4. Coalesce duplicate signals; assign generation tokens so late work from a prior refresh cannot overwrite a manual command or newer resume.
5. Avoid full scene uploads when the same content remains selected; allow documented re-upload only when resources need recovery.
6. Expose degraded state if the service is missing/disconnected; keep rendering/manual reload working. A large wall-clock jump is not proof of resume.
7. Live-test several sleep/wake cycles with offline weather, an output topology change during sleep, and a failed candidate scene. Ask the operator before suspending their active session.

## Acceptance criteria

- [ ] **S08-AC1:** Synthetic and live traces show submission pause on sleep and safe resumption only when output configuration is valid.
- [ ] **S08-AC2:** One coalesced resume produces one selection; duplicate signals and late weather/scene work do not cause extra swaps.
- [ ] **S08-AC3:** Same-scene resume avoids redundant full uploads unless GPU recovery requires them; stale weather or candidate failure preserves a working scene.
- [ ] **S08-AC4:** Missing/disconnected power service is reported clearly without a crash or false resume claim; manual refresh remains usable internally.
- [ ] **S08-AC5:** Real multi-cycle suspend evidence is recorded; reviewer accepts M3 with S06–S08 gates, not just synthetic signal tests.

## Failure and rollback

Pause/recovery must not fabricate a healthy device after failure. Retain valid resources or exit through the documented fatal path. If real suspend cannot be tested, record a validation roadblock and keep acceptance open; never suspend an unattended user's machine without permission.

## Handoff

Provide signal/version/permission details, state-machine traces, coalescing policy, timing behavior, and live cycle evidence. S09 extends output handling without adding new scene-selection triggers.
