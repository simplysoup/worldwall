# Delivery roadmap

**Baseline:** approved contract v0.8; original v0.1 ratified by `simplysoup` on 2026-09-21; CR-0008 applied 2026-09-23. This is the only stage-state/owner ledger. Stage details define gates, not progress.

## Stage ledger

| Stage | Delivery | Dependencies | State | Owner | Acceptance evidence |
| --- | --- | --- | --- | --- | --- |
| [S00](stages/00-foundation.md) | Foundation and baseline approval | None | accepted | Pi coding agent | [S00 foundation increment](evidence/2026-09-21-s00-foundation.md) |
| [S01](stages/01-wayland-surface.md) | Single-output Wayland lifecycle | S00 | accepted | Pi coding agent | [S01 lifecycle increment](evidence/2026-09-21-s01-wayland-lifecycle.md) |
| [S02](stages/02-gpu-presentation.md) | Safe GPU presentation | S01 | accepted | Pi coding agent | [S02 review and acceptance](evidence/2026-09-21-s02-gpu-presentation.md) |
| [S03](stages/03-indexed-scene.md) | Static indexed scene / M1 | S02 | accepted | Pi coding agent | [S03 indexed-scene evidence](evidence/2026-09-21-s03-indexed-scene.md) |
| [S04](stages/04-palette-animation.md) | GPU animation and pacing | S03 | accepted | Pi coding agent | [S04 implementation and acceptance](updates/2026-09-21-s04-palette-animation.md); [CR-0001 BlendShift increment](updates/2026-09-22-s04-byte-blendshift.md); [CR-0002 timeline increment](updates/2026-09-22-cr002-time-of-day.md); [CR-0005 implementation](updates/2026-09-22-cr0005-config-location-solar.md); [CR-0005 test batch](updates/2026-09-23-cr0005-test-batch-run-guide.md); CR-0007 [change](changes/CR-0007-global-animation-speed-multiplier.md) and [implementation update](updates/2026-09-23-cr0007-animation-speed-multiplier.md); [partial live rate evidence](updates/2026-09-23-hyprland-session-activation.md) |
| [S05](stages/05-scene-registry.md) | Registry and selection / M2 | S04 | accepted | Pi coding agent | Historical [S05 review submission](evidence/2026-09-22-s05-registry-selection.md); CR-0006 addendum [update](updates/2026-09-23-cr0006-weather-adaptive-scenes.md); [RB-0004](roadblocks/RB-0004-cr0006-adaptive-artwork-clarification.md); RB-0005 [resolution](roadblocks/RB-0005-scene-loading-blocks-wayland-owner.md) and [implementation update](updates/2026-09-23-rb0005-bounded-loading.md); [partial live registry/selection evidence](updates/2026-09-23-hyprland-session-activation.md) |
| [S06](stages/06-time-astronomy.md) | Civil time and astronomy | S05 | accepted | Pi coding agent | [S06 time/astronomy acceptance](evidence/2026-09-22-s06-time-astronomy.md); [CR-0002 timeline increment](updates/2026-09-22-cr002-time-of-day.md); [CR-0005 implementation](updates/2026-09-22-cr0005-config-location-solar.md); [CR-0005 test batch](updates/2026-09-23-cr0005-test-batch-run-guide.md) |
| [S07](stages/07-weather.md) | Optional async weather cache | S06 | accepted | Pi coding agent | Historical [S07 failure-path evidence](updates/2026-09-22-s07-failure-gates.md); [CR-0005 evidence](updates/2026-09-23-cr0005-test-batch-run-guide.md); [CR-0006 addendum](updates/2026-09-23-cr0006-weather-adaptive-scenes.md); [partial live weather evidence](updates/2026-09-23-hyprland-session-activation.md) |
| [S08](stages/08-suspend-resume.md) | Suspend/resume / M3 | S07 | accepted | Pi coding agent | Historical [S08 acceptance](updates/2026-09-22-s08-suspend-resume.md); [CR-0006 addendum](updates/2026-09-23-cr0006-weather-adaptive-scenes.md); [session logind connection, physical resume not-run](updates/2026-09-23-hyprland-session-activation.md) |
| [S09](stages/09-multi-output.md) | Multi-output and hotplug | S08 | accepted | Pi coding agent | [S09 implementation and acceptance](updates/2026-09-22-s09-multi-output.md) |
| [S10](stages/10-ipc-config.md) | Local IPC and config transactions | S09 | accepted | Pi coding agent | Historical [S10 acceptance](updates/2026-09-22-s10-ipc-config.md); [CR-0005 evidence](updates/2026-09-23-cr0005-test-batch-run-guide.md); CR-0006 addendum [update](updates/2026-09-23-cr0006-weather-adaptive-scenes.md); CR-0007 [change](changes/CR-0007-global-animation-speed-multiplier.md) and [implementation update](updates/2026-09-23-cr0007-animation-speed-multiplier.md); RB-0005 [resolution](roadblocks/RB-0005-scene-loading-blocks-wayland-owner.md) and [implementation update](updates/2026-09-23-rb0005-bounded-loading.md); [partial live config-rate reload evidence](updates/2026-09-23-hyprland-session-activation.md) |
| [S11](stages/11-release-validation.md) | Recovery, performance, release / M4 | S10 | accepted | Pi coding agent | [S11 M4 acceptance](updates/2026-09-22-s11-m4-acceptance.md); evidence: [release package](updates/2026-09-22-s11-release-packaging.md), [benchmarks](updates/2026-09-22-s11-benchmark-pmon-reruns.md), [dispositions](updates/2026-09-22-s11-result-review-disposition.md), [clean checkout](updates/2026-09-22-s11-clean-checkout-ac1.md), [recovery matrix](updates/2026-09-22-s11-ac2-recovery-matrix.md), [docs/licensing](updates/2026-09-22-s11-ac4-ac5-docs-licensing.md); [opt-in session deployment, not new acceptance](updates/2026-09-23-hyprland-session-activation.md) |
| [S12](stages/12-scene-pool-curator.md) | Local scene-pool curator and automatic eligibility / post-M4 | S04, S05, S06, S07, S08, S10, S11 | in-progress | Pi coding agent | [CR-0008 baseline application](updates/2026-09-23-cr0008-baseline-application.md); [AC1 approved policy and Firefox host spike](updates/2026-09-23-s12-ac1-kickoff.md) (accepted by `simplysoup`, 2026-09-23); [AC2 local capture importer](updates/2026-09-23-s12-ac2-local-capture-importer.md) and [catalog/sidecar model](updates/2026-09-23-s12-ac2-catalog-sidecar-model.md) slices (AC2 accepted by `simplysoup`, 2026-09-23; only one local capture available); [AC3 partial read-only curator slice](updates/2026-09-23-s12-ac3-curator-readonly.md) |

`ready` means dependencies/inputs permit starting work, **not** that approval or delivery is complete. S03 is accepted with ADR-0003 and recorded pure/GPU/live evidence. CR-0005's historical acceptance remains recorded. The stage ledger preserves the original S00–S11 acceptances; CR-0006/CR-0007 addendum gates are tracked separately and are not accepted by those historical states. [RB-0004](roadblocks/RB-0004-cr0006-adaptive-artwork-clarification.md) records the clarified, private loader-validated crop but does not establish adaptive acceptance. [RB-0005](roadblocks/RB-0005-scene-loading-blocks-wayland-owner.md) is resolved at the bounded async implementation/deterministic-test level; S05/S10 acceptance and assigned-session adaptive evidence remain separate. The linked updates distinguish offline tests and partial live activation evidence from the remaining assigned-session gates and not-run next-login test.

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

No multi-device work, extra scaling modes, cross-family weather reselection, or implicit weather activation is scheduled. CR-0008 adds the approved S12 curator/pool-filtering extension after M4, without changing selection triggers; implementation and acceptance remain open. CR-0006's approved same-family precipitation-variant behavior is opt-in; richer user-facing family/threshold configuration remains deferred.

## Mapping from the original plan

Original Phases 1–10 map to S01–S10. S00 adds reproducibility/decision setup. S11 removes release recovery/performance/packaging from the vague final “polish” bucket. Original MVP acceptance is reached at S03, not merely at creation of a layer role or presentation of a solid color. See [audit A01/A13](audit.md).
