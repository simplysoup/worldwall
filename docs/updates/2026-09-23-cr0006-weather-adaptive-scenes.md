# CR-0006 policy and implementation update — 2026-09-23

- Baseline: approved CR-0006; contract v0.6 policy applied in the uncommitted worktree based on `c5263e7`.
- Approval: `simplysoup`, 2026-09-23. This update does not accept any stage or rollout gate.
- Requirements: WW-06/07/08/10; S05-AC3/4/6/7; S07-AC1/4/7; S08-AC2/3/6; S10-AC3/7/8.

## Changes reviewed

- `src/scene_registry.rs`, `src/config.rs`, `src/selection_state.rs`, `src/main.rs`, and `src/wayland_surface.rs` contain the CR-0006 adaptive policy: explicit family/band metadata, approved thresholds with 2°C hysteresis, deterministic full selection, same-family precipitation fallbacks, schema-v3 default-off opt-in, private family/band-only state, background weather/resume candidate loading, and the existing transactional renderer-swap boundary.
- Code review found two stale-completion races and added guards: successful config reload invalidates queued adaptive scene candidates without advancing visible selection generation; a newer full-selection trigger resolving to the resident scene invalidates an older pending family candidate. Fresh weather arriving during a full selection is reconsidered only as a same-family variant after the full candidate completes.
- Canonical policy is reconciled in `docs/delivery-contract.md` v0.6, `docs/scene-format.md`, `docs/ipc.md`, `docs/architecture.md`, S05/S07/S08/S10 criteria, `docs/release.md`, `examples/worldwall.toml`, `docs/roadmap.md`, `docs/status.md`, and `README.md`. Historical CR-0005/S05/S07/S08/S10 evidence records were not rewritten.
- RB-0004 records the open Monolith crop rectangle and V29 cloudy BlendShift speed questions. Source artwork, private derived assets, private operator config, and the startup hook were not changed.

## Validation and limitations

| Check | Result |
| --- | --- |
| `cargo fmt --all --check` | pass |
| `cargo clippy --offline --all-targets --all-features -- -D warnings` | pass; an unused test accessor was restricted to `cfg(test)` after the first lint run surfaced it |
| `cargo test --offline --all-targets --all-features` | pass: main 106 passed, 0 failed, 3 ignored GPU readback tests; CLI 4 passed |
| `cargo build --offline --locked --all-targets --all-features` | pass |
| `python3 tools/check_docs.py` | pass: 115 Markdown files, 12 stages, 7 skills; archive intact |
| `python3 -m unittest discover -s tools/tests -v` | pass: 49 tests |
| `git diff --check` | pass |
| Assigned-session adaptive Wayland/Vulkan swap | not run |
| Provider/network request, Worldwall launch/restart, next-login test | not run |

A blocking implementation issue is recorded as [RB-0005](../roadblocks/RB-0005-scene-loading-blocks-wayland-owner.md) for S05-AC4/S10-AC8: the adaptive `reload` IPC path currently calls `SceneManifest::load_scene()` synchronously in `handle_ipc_requests`; existing manual scene/next paths also load synchronously, and config reload validates/decodes the registry on the event-loop owner. This does not meet the required off-Wayland-thread scene/config loading boundary. The path must be refactored and tested before claiming CR-0006 acceptance. The targeted unit tests exercise policy and invalidation, not this runtime threading behavior.

The Monolith crop boundary cannot be safely inferred from its index/palette edges. The cloudy V29 animation-speed intent remains unknown. Until both are clarified, no complete private adaptive registry can be built or activated and no assigned-session adaptive-swap evidence can be gathered. Next-login behavior remains untested.

## Next safe actions

1. Obtain the exact Monolith crop rectangle/edge convention and intended V29 cloudy speed treatment from `simplysoup`; keep original fixtures unchanged.
2. Refactor registry candidate/config loading so no scene decoding blocks Wayland event handling; retain command transaction/error semantics and add deterministic stale/queue-full tests.
3. After the async-path fix, rerun the offline Rust, documentation, tooling, and diff checks; then pursue only separately authorized live evidence. Do not launch Worldwall, fetch weather, activate private config, edit private artwork, test next login, or commit in this phase.
