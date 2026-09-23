# CR-0007 implementation update — 2026-09-23

- Baseline: maintainer-directed CR-0007 applied to delivery contract v0.7 in the uncommitted worktree based on `c5263e7`.
- Approval: `simplysoup`, 2026-09-23. This update does not accept S04/S10 or live behavior.
- Requirements: WW-04/10; S04-AC9; S10-AC9.

## Changes

- Added schema-v4 top-level `animation_speed_multiplier` with range `0.0..=1.0` and default `0.5`. Config schema-v1/v2/v3 remain readable and receive the default when the new field is absent; explicit overrides require schema v4.
- Reworked `PausedAnimationClock` to rate-integrate monotonic elapsed time, wrap its bounded shader phase, preserve phase when the multiplier changes, and continue excluding suspend duration. The multi-output runtime applies the validated config rate before subsequent submissions; civil/solar palette sampling remains separate.
- Added config tests for default/legacy behavior, schema-v4 parsing, range/non-finite rejection, and unsupported old-schema override. Added clock tests for half speed, pause/resume, and phase-continuous multiplier changes; extended config-transaction coverage to confirm a new rate commits without scene generation changes.
- Reconciled contract v0.7, CR-0007, the change register, S04/S10 supplemental criteria, IPC/config, architecture, scene format, example config, README, roadmap, status, and RB-0004/RB-0005 wording.
- Used the user-supplied red guide to prepare a private Monolith Plains review crop in a private review package outside the repository. The reproducible half-open source rectangle is `(18, 30, 346, 476)`, yielding a 328×446 8-bit grayscale/index image; palette, time-of-day, and source animation files are copied unchanged. The source scene and guide were not modified. This package is not activated or added to the repository.
- Replaced the private V29 copy's per-file halved animation manifest with the source-speed manifest so global `0.5` is applied once. The prior manifest was preserved in a private state backup outside the repository; its SHA-256 is `68124ed350c5d9db6cb6e54308f6dd5f06d79e7ecf602ca912631944dd54b5f7`. The private operator config was not changed.

## Validation and limitations

| Check | Result |
| --- | --- |
| `cargo fmt --all --check` | pass |
| `cargo test --offline --all-targets --all-features` | pass: main 109 passed, 0 failed, 3 Vulkan readback tests ignored; CLI 4 passed |
| `cargo clippy --offline --all-targets --all-features -- -D warnings` | pass |
| `cargo build --offline --locked --all-targets --all-features` | pass |
| `python3 tools/check_docs.py` | pass: 117 Markdown files, 12 stages, 7 skills |
| `python3 -m unittest discover -s tools/tests -v` | pass: 49 tests |
| `git diff --check` | pass |
| Private crop integrity check | pass: TOML parsed; pixels match the half-open source rectangle; copied palette, animation, and time-of-day resources are byte-identical |
| Assigned-session visual/pacing or live `reload-config` check | not run |
| Worldwall launch/restart, network request, adaptive config activation, next-login test | not run |

RB-0004 is resolved as a clarification/input record, not as adaptive artwork or rollout acceptance. The private crop passes TOML/pixel/resource integrity checks; `simplysoup` approved its visual appearance on 2026-09-23, while application-loader validation remains pending. RB-0005 remains open: explicit IPC scene loading and registry/config validation still include synchronous decode on the Wayland event owner. The trigger semantics are implementable; this is an async responsiveness gate, not a missing trigger design. Do not claim CR-0006 or CR-0007 acceptance based on unit tests or the review image.

## Next safe actions

1. Complete application-loader validation of the visually approved private crop before placing it in an adaptive registry.
2. Refactor/test bounded background validation and loading before claiming RB-0005/S05-AC4/S10-AC8; preserve IPC/config transaction semantics.
3. Pursue live animation/adaptive evidence or next-login work only after the relevant separate authorization.
