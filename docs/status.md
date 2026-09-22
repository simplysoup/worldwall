# Current project status

**As of:** 2026-09-21 · **Evidence basis:** S00–S03 acceptance, accepted ADR-0002/ADR-0003/ADR-0004 reviews, S04 animated GPU readback, assigned-session live lifecycle traces, and the four-workload resource baseline; S04 is accepted.

## Summary

Worldwall has implemented and accepted S01 single-output Wayland lifecycle, S02 safe GPU presentation, and S03 static indexed-scene/M1 work. S02 waits for SCTK configure, creates a Vulkan wgpu surface on the retained layer's exact `wl_surface`, derives FIFO configuration from capabilities, requests a frame callback before presenting a solid fullscreen triangle, handles live resize, and preserves owner-safe teardown ordering. `simplysoup` is the assigned maintainer/reviewer and `soupydesktop` is the assigned live validation host. The delivery contract and ADR-0001 baseline were ratified by `simplysoup` on 2026-09-21.

**Accepted software stages: 5 of 12; S00, S01, S02, S03, and S04 are accepted.** S04's accepted ADR-0004, animated GPU, pacing, bounded live lifecycle, resize, and observational resource-baseline evidence are recorded in the linked update. See the [canonical ledger](roadmap.md), [S04 implementation update](updates/2026-09-21-s04-palette-animation.md), [ADR-0004](decisions/ADR-0004-s04-animation.md), [S03 evidence](evidence/2026-09-21-s03-indexed-scene.md), [S02 evidence](evidence/2026-09-21-s02-gpu-presentation.md), [S01 evidence](evidence/2026-09-21-s01-wayland-lifecycle.md), and [S00 evidence](evidence/2026-09-21-s00-foundation.md).

## Advice and next action

1. Keep the accepted S01–S04 evidence as the baseline; preserve the renderer and pacing boundaries while starting only the approved S05 registry/selection work.
2. Preserve the S01/S02 nested harnesses and locked-check outputs as regression checks; keep the no-bundled-art-assets rule.
3. S05 may begin registry/selection work; do not introduce environment-driven reselection or alter the accepted S04 renderer contract.
4. Keep unsupported or untested behavior visible. Do not substitute screenshots for lifecycle tests or claim energy savings without a repeatable baseline.

## Decisions and blockers

- The maintainer/reviewer is GitHub account `simplysoup`; ADR-0001, ADR-0002, ADR-0003, S00, S01, S02, and S03 were accepted on 2026-09-21. `simplysoup` instructed that unqualified future review approvals in this project should be recorded under that identity unless otherwise specified. `simplysoup` assigned `soupydesktop` as the live Hyprland/Vulkan validation host. Code/docs are MIT-licensed and art assets are not bundled; S02 ownership is accepted in [ADR-0002](decisions/ADR-0002-s02-wayland-wgpu-ownership.md). See the [decision register](decisions/README.md), [S00 evidence](evidence/2026-09-21-s00-foundation.md), [S01 evidence](evidence/2026-09-21-s01-wayland-lifecycle.md), [S02 evidence](evidence/2026-09-21-s02-gpu-presentation.md), and [S03 evidence](evidence/2026-09-21-s03-indexed-scene.md).
- The local environment exposes Hyprland 0.56.2, Vulkan 1.4.357, an NVIDIA RTX 3070 Ti, and two scale-1 1920×1080 outputs, but this observation is not an assigned acceptance record.
- ADR-0003's scene decoding/color rules and S03/M1 were accepted by `simplysoup` on 2026-09-21. `simplysoup` accepted ADR-0004's S04 animation authoring/GPU-packing decision and accepted S04 on 2026-09-21 after reviewing the recorded implementation evidence. At the user's request, three Effect Games example scene scripts were fetched and materialized as local untracked provenance fixtures only; no art or pixel data was added to the tracked delivery. See [ADR-0004](decisions/ADR-0004-s04-animation.md) and the external manifest at `/home/soup/.local/state/worldwall/s04-implementation-20260921/effectgames-example-sources.json`. Derived source fixtures are deliberately local and untracked under `fixtures/untracked/effectgames/`; they are not part of the tracked or published delivery.
- Provider, location, deadlines, and stale-weather policy must close before S07; numerical release budgets before S11 benchmarking.
- **Confirmed execution roadblocks: none recorded.** The S02 nested harness now verifies a distinct child socket and both teardown paths; the remaining debug-only fault seam is documented as a test aid, not a release behavior. Open a [roadblock](roadblocks/README.md) only when a specific assigned task cannot proceed.

## Verification and limitations

See the [planning baseline update](updates/2026-09-21-planning-baseline.md), [S00 evidence](evidence/2026-09-21-s00-foundation.md), [S01 evidence](evidence/2026-09-21-s01-wayland-lifecycle.md), [S02 evidence](evidence/2026-09-21-s02-gpu-presentation.md), and [S03 evidence](evidence/2026-09-21-s03-indexed-scene.md) for checks and artifacts. S02's live evidence covers visible ordinary-app/input behavior using virtual input events, real output resize/restore, the reviewed debug fault matrix, and distinct-socket output-removal/disconnect teardown. The evidence does not claim a physical GPU fault or universal hardware compatibility beyond the recorded host.

Update this short summary after meaningful increments or decisions. Keep detailed logs in evidence/updates; keep stage state and ownership in the roadmap.
