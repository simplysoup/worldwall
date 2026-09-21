# Current project status

**As of:** 2026-09-21 · **Evidence basis:** S00 foundation build/API checks, local session inventory, and documentation checks; no renderer execution.

## Summary

Worldwall has begun S00 foundation work. A minimal Rust binary, locked dependency graph, toolchain pin, API smoke target, fixture plan, MIT license, and local build/check instructions now exist. `simplysoup` is the assigned maintainer/reviewer. The repository now has local Git history on `main` with origin set to `https://github.com/simplysoup/worldwall`; it still has no packaged artwork, renderer, or release. The delivery contract and ADR-0001 baseline were ratified by `simplysoup` on 2026-09-21.

**Accepted software stages: 0 of 12; S00 is in-progress.** See the [canonical ledger](roadmap.md) and [S00 evidence](evidence/2026-09-21-s00-foundation.md); the passing smoke checks do not prove Wayland presentation or renderer safety. No GPU rendering, teardown, suspend, weather, or performance acceptance tests have run.

## Advice and next action

1. `simplysoup` should assign the live-host operator and complete the S00 review record.
2. Retain the final locked-check outputs and preserve the no-bundled-art-assets rule.
3. Work through S01–S03 before weather or broad module scaffolding. The first valuable demo is a correctly scaled static indexed image with safe resize and teardown.
4. Measure pacing/resource behavior in S04 and keep those measurements as a regression baseline.
5. Keep unsupported or untested behavior visible. Do not substitute screenshots for lifecycle tests or claim energy savings without a repeatable baseline.

## Decisions and blockers

- The maintainer/reviewer is GitHub account `simplysoup`; ADR-0001 and the delivery baseline were ratified on 2026-09-21, while live test-host/operator assignment remains an open S00 task. Code/docs are MIT-licensed and art assets are not bundled; see the [decision register](decisions/README.md) and [S00 evidence](evidence/2026-09-21-s00-foundation.md).
- The local environment exposes Hyprland 0.56.2, Vulkan 1.4.357, an NVIDIA RTX 3070 Ti, and two scale-1 1920×1080 outputs, but this observation is not an assigned acceptance record.
- Scene decoding/color rules must close before S03; animation packing must close before S04.
- Provider, location, deadlines, and stale-weather policy must close before S07; numerical release budgets before S11 benchmarking.
- **Confirmed execution roadblocks: none recorded.** No implementation was attempted. Missing future capabilities are not discovered defects. Open a [roadblock](roadblocks/README.md) only when a specific assigned task cannot proceed.

## Verification and limitations

See the [planning baseline update](updates/2026-09-21-planning-baseline.md) for checks and artifacts. Python documentation checks do not validate Rust APIs, Wayland lifetimes, the proposed scene schema implementation, GPU output, or resource use. All such gates remain open.

Update this short summary after meaningful increments or decisions. Keep detailed logs in evidence/updates; keep stage state and ownership in the roadmap.
