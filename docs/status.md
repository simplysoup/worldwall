# Current project status

**As of:** 2026-09-21 · **Evidence basis:** S00–S03 acceptance, accepted ADR-0002/ADR-0003 reviews, locked checks, GPU/reference readback, live input/resize/fault traces, and distinct-socket teardown harness results.

## Summary

Worldwall has implemented and accepted S01 single-output Wayland lifecycle, S02 safe GPU presentation, and S03 static indexed-scene/M1 work. S02 waits for SCTK configure, creates a Vulkan wgpu surface on the retained layer's exact `wl_surface`, derives FIFO configuration from capabilities, requests a frame callback before presenting a solid fullscreen triangle, handles live resize, and preserves owner-safe teardown ordering. `simplysoup` is the assigned maintainer/reviewer and `soupydesktop` is the assigned live validation host. The delivery contract and ADR-0001 baseline were ratified by `simplysoup` on 2026-09-21.

**Accepted software stages: 4 of 12; S00, S01, S02, and S03 are accepted.** S03-AC1 through S03-AC5 passed under accepted ADR-0003, with decoder tests, GPU/reference readback, and live textured lifecycle evidence. See the [canonical ledger](roadmap.md), [S03 evidence](evidence/2026-09-21-s03-indexed-scene.md), [S02 evidence](evidence/2026-09-21-s02-gpu-presentation.md), [S01 evidence](evidence/2026-09-21-s01-wayland-lifecycle.md), and [S00 evidence](evidence/2026-09-21-s00-foundation.md).

## Advice and next action

1. Keep the accepted S01–S03 evidence as the M1 baseline; before S04 implementation, close its animation schema/GPU-packing decision.
2. Preserve the S01/S02 nested harnesses and locked-check outputs as regression checks; keep the no-bundled-art-assets rule.
3. Complete S01–S03 before weather or broad module scaffolding. The first valuable demo is a correctly scaled static indexed image with safe resize and teardown.
4. Measure pacing/resource behavior in S04 and keep those measurements as a regression baseline.
5. Keep unsupported or untested behavior visible. Do not substitute screenshots for lifecycle tests or claim energy savings without a repeatable baseline.

## Decisions and blockers

- The maintainer/reviewer is GitHub account `simplysoup`; ADR-0001, ADR-0002, ADR-0003, S00, S01, S02, and S03 were accepted on 2026-09-21. `simplysoup` instructed that unqualified future review approvals in this project should be recorded under that identity unless otherwise specified. `simplysoup` assigned `soupydesktop` as the live Hyprland/Vulkan validation host. Code/docs are MIT-licensed and art assets are not bundled; S02 ownership is accepted in [ADR-0002](decisions/ADR-0002-s02-wayland-wgpu-ownership.md). See the [decision register](decisions/README.md), [S00 evidence](evidence/2026-09-21-s00-foundation.md), [S01 evidence](evidence/2026-09-21-s01-wayland-lifecycle.md), [S02 evidence](evidence/2026-09-21-s02-gpu-presentation.md), and [S03 evidence](evidence/2026-09-21-s03-indexed-scene.md).
- The local environment exposes Hyprland 0.56.2, Vulkan 1.4.357, an NVIDIA RTX 3070 Ti, and two scale-1 1920×1080 outputs, but this observation is not an assigned acceptance record.
- ADR-0003's scene decoding/color rules and S03/M1 were accepted by `simplysoup` on 2026-09-21. Animation packing must close before S04.
- Provider, location, deadlines, and stale-weather policy must close before S07; numerical release budgets before S11 benchmarking.
- **Confirmed execution roadblocks: none recorded.** The S02 nested harness now verifies a distinct child socket and both teardown paths; the remaining debug-only fault seam is documented as a test aid, not a release behavior. Open a [roadblock](roadblocks/README.md) only when a specific assigned task cannot proceed.

## Verification and limitations

See the [planning baseline update](updates/2026-09-21-planning-baseline.md), [S00 evidence](evidence/2026-09-21-s00-foundation.md), [S01 evidence](evidence/2026-09-21-s01-wayland-lifecycle.md), [S02 evidence](evidence/2026-09-21-s02-gpu-presentation.md), and [S03 evidence](evidence/2026-09-21-s03-indexed-scene.md) for checks and artifacts. S02's live evidence covers visible ordinary-app/input behavior using virtual input events, real output resize/restore, the reviewed debug fault matrix, and distinct-socket output-removal/disconnect teardown. The evidence does not claim a physical GPU fault or universal hardware compatibility beyond the recorded host.

Update this short summary after meaningful increments or decisions. Keep detailed logs in evidence/updates; keep stage state and ownership in the roadmap.
