# Delivery roadmap

**Baseline:** approved v0.1, ratified by `simplysoup` on 2026-09-21. This is the only stage-state/owner ledger. Stage details define gates, not progress.

## Stage ledger

| Stage | Delivery | Dependencies | State | Owner | Acceptance evidence |
| --- | --- | --- | --- | --- | --- |
| [S00](stages/00-foundation.md) | Foundation and baseline approval | None | accepted | Pi coding agent | [S00 foundation increment](evidence/2026-09-21-s00-foundation.md) |
| [S01](stages/01-wayland-surface.md) | Single-output Wayland lifecycle | S00 | accepted | Pi coding agent | [S01 lifecycle increment](evidence/2026-09-21-s01-wayland-lifecycle.md) |
| [S02](stages/02-gpu-presentation.md) | Safe GPU presentation | S01 | accepted | Pi coding agent | [S02 review and acceptance](evidence/2026-09-21-s02-gpu-presentation.md) |
| [S03](stages/03-indexed-scene.md) | Static indexed scene / M1 | S02 | accepted | Pi coding agent | [S03 indexed-scene evidence](evidence/2026-09-21-s03-indexed-scene.md) |
| [S04](stages/04-palette-animation.md) | GPU animation and pacing | S03 | accepted | Pi coding agent | [S04 implementation and acceptance](updates/2026-09-21-s04-palette-animation.md); [CR-0001 BlendShift increment](updates/2026-09-22-s04-byte-blendshift.md); [CR-0002 timeline increment](updates/2026-09-22-cr002-time-of-day.md) |
| [S05](stages/05-scene-registry.md) | Registry and selection / M2 | S04 | accepted | Pi coding agent | [S05 review submission](evidence/2026-09-22-s05-registry-selection.md) |
| [S06](stages/06-time-astronomy.md) | Civil time and astronomy | S05 | accepted | Pi coding agent | [S06 time/astronomy acceptance](evidence/2026-09-22-s06-time-astronomy.md); [CR-0002 timeline increment](updates/2026-09-22-cr002-time-of-day.md) |
| [S07](stages/07-weather.md) | Optional async weather cache | S06 | accepted | Pi coding agent | [S07 failure-path and acceptance evidence](updates/2026-09-22-s07-failure-gates.md) |
| [S08](stages/08-suspend-resume.md) | Suspend/resume / M3 | S07 | accepted | Pi coding agent | [S08 implementation and acceptance](updates/2026-09-22-s08-suspend-resume.md) |
| [S09](stages/09-multi-output.md) | Multi-output and hotplug | S08 | accepted | Pi coding agent | [S09 implementation and acceptance](updates/2026-09-22-s09-multi-output.md) |
| [S10](stages/10-ipc-config.md) | Local IPC and config transactions | S09 | accepted | Pi coding agent | [S10 IPC/config implementation and acceptance](updates/2026-09-22-s10-ipc-config.md) |
| [S11](stages/11-release-validation.md) | Recovery, performance, release / M4 | S10 | accepted | Pi coding agent | [S11 M4 acceptance](updates/2026-09-22-s11-m4-acceptance.md); evidence: [release package](updates/2026-09-22-s11-release-packaging.md), [benchmarks](updates/2026-09-22-s11-benchmark-pmon-reruns.md), [dispositions](updates/2026-09-22-s11-result-review-disposition.md), [clean checkout](updates/2026-09-22-s11-clean-checkout-ac1.md), [recovery matrix](updates/2026-09-22-s11-ac2-recovery-matrix.md), [docs/licensing](updates/2026-09-22-s11-ac4-ac5-docs-licensing.md) |

`ready` means dependencies/inputs permit starting work, **not** that approval or delivery is complete. S03 is accepted with ADR-0003 and recorded pure/GPU/live evidence; S04 is accepted with ADR-0004 and the recorded implementation evidence, with the additive CR-0001 byte-space BlendShift amendment applied and evidenced.

## State machine

`planned → ready → in-progress → review → accepted`

- `planned`: waiting for dependencies or required inputs.
- `ready`: dependencies accepted, a bounded task can be claimed. Integrator assigns owner on claim.
- `in-progress`: an owner is actively executing the stage with a task/evidence record.
- `blocked`: required work cannot proceed; link a roadblock with owner and next check. Return to `ready` or `in-progress` on verified resolution.
- `review`: implementation and gate evidence submitted; not yet accepted.
- `accepted`: assigned acceptance authority reviewed all criteria, dependencies, and evidence. Record reviewer/date in the evidence linked here.

If new evidence invalidates an accepted gate, retain historical acceptance and append a correction; move the stage to `review` or `blocked`, mark dependent acceptance stale, and stop release claims. Do not rewrite history to hide the regression.

Routine state/owner/evidence changes use the progress workflow. Reordering, scope changes, dropping gates, and adding stages use a [change proposal](workflows/roadmap-changes.md).

## Dependencies and parallel work

The default critical path is deliberately linear: prove the renderer before integrating services. Delegates may prepare fixtures, pure-function tests, or bounded design spikes ahead of integration when their inputs are settled. Such preparation does not accept a future stage or bypass dependency gates. The integrator assigns non-overlapping files; shared renderer interfaces, roadmap, and status have one writer per increment.

No multi-device work, extra scaling modes, live weather reselection, or GUI editor is scheduled. Deferred features stay out of the critical path unless approved.

## Mapping from the original plan

Original Phases 1–10 map to S01–S10. S00 adds reproducibility/decision setup. S11 removes release recovery/performance/packaging from the vague final “polish” bucket. Original MVP acceptance is reached at S03, not merely at creation of a layer role or presentation of a solid color. See [audit A01/A13](audit.md).
