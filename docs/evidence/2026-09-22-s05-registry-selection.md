# Evidence — S05 registry and transactional selection review submission

- Date / operator: 2026-09-22 / Pi coding agent
- Revision or artifact hashes: no Git commit; source artifact hashes are listed below
- Stage / requirements: S05; WW-03, WW-06, WW-11
- Environment: Linux 7.2.4-zen2-1-zen x86_64; Rust 1.94.0; Cargo 1.94.0; Hyprland 0.56.2; Vulkan instance 1.4.357; local NVIDIA/Vulkan session. This is observational local evidence, not assigned-host acceptance.
- Inputs: generated synthetic indexed PNG packages for unit tests; two local untracked animated provenance fixtures for the M2 startup trace; no network, location, weather, or bundled art assets
- Kind: pure unit, headless GPU, live local Wayland/Vulkan startup, and documentation validation

## Results

| Criterion | Exact command or manual procedure | Expected | Observed | Outcome | Artifact |
| --- | --- | --- | --- | --- | --- |
| S05-AC1 | `cargo test --locked --all-targets --all-features` | Reject duplicate IDs/tags, invalid tags/versions/assets, missing fallback, and path escapes; retain a valid active scene after candidate failure | Registry/package tests reject malformed manifests, unsupported schema, duplicate/unsupported tags, missing/bad fallback, missing assets, and traversal; transaction failure test retains the prior scene | pass | `src/scene_registry.rs` tests; source hash below |
| S05-AC2 | Same locked unit command | Fixed snapshot/seed is repeatable; manual, next, fallback, and empty-registry policy is deterministic | Repeatable seeded scoring, unknown/fallback, manual ID, compatible next exclusion, no-compatible and empty-registry cases pass | pass | `src/scene_registry.rs` tests |
| S05-AC3 | Same locked unit command; inspect `SelectionTrigger::is_explicit` and controller generation tests | Only startup/resume/reload/manual/next advance selection generation | Weather, clock, output, and config triggers all return `Ignored` and leave generation unchanged; startup loading uses an explicit `Startup` request | pass | `src/scene_registry.rs`; `src/wayland_surface.rs` |
| S05-AC4 | Same locked unit command; local renderer startup; inspect `SceneTransaction::complete_with_swap` and `Renderer::upload_and_swap_scene` | Stale work cannot win; failed/unchanged candidates retain active resources; changed candidates swap atomically | Stale completion never calls the swap callback; unchanged content does not call it; load and simulated renderer-swap failures retain the active scene. Local startup kept one upload through 54 frames | pass for software boundary; live explicit swap not run | `src/scene_registry.rs`; `$HOME/.local/state/worldwall/s05-implementation-20260921/registry-startup.log` |
| S05-AC5 | `cargo test --locked --all-targets --all-features -- --ignored`; two-animated-package registry startup with `WORLDWALL_TEST_EXIT_AFTER_MS=1800 cargo run --locked` | Multiple animated packages load through the same renderer contract without renderer-specific scene logic | Animated hue and brightness fixtures loaded through the registry tests; both ignored GPU readback tests passed; local registry loaded two animated packages, selected `forest`, presented animated frames, and kept `scene-upload-count=1` | pass for implementation/demo evidence; reviewer acceptance pending | `$HOME/.local/state/worldwall/s05-implementation-20260921/m2-two-animated-registry.log` |

## Validation commands

| Check | Result |
| --- | --- |
| `cargo fmt --check` | pass |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | pass |
| `cargo test --locked --all-targets --all-features` | pass — 40 passed, 2 ignored |
| `cargo test --locked --all-targets --all-features -- --ignored` | pass — 2 GPU readback tests passed |
| `python3 tools/check_docs.py` | pass — 69 Markdown files, 12 stages, 7 skills |
| `python3 -m unittest discover -s tools/tests -v` | pass — 33 tests |
| `git diff --check` | pass |

## Source artifact hashes

- `src/scene_registry.rs` — `d5e16142754688b16e9b3cb953b243dacef2ab51d347e912416a3726375cc648`
- `src/wayland_surface.rs` — `7172502010115f680215393e697f404bfe991c63e90b786de821c7b9a26bfcf8`
- `src/renderer.rs` — `229a08411e57b3e7a7bdc245b9cb892bbadb43d34002f215b748c4050564d63d`

## Limitations and failures

The S05 deliverable intentionally has no public IPC. The explicit begin/complete
handoff is an internal entrypoint for a later event source: it returns a
cloneable manifest so loading can occur away from Wayland dispatch, then
completes at the renderer boundary. No timer, output, weather, or ordinary
config callback invokes selection. The local live trace exercises registry
startup and animated rendering, not a live manual scene command or a forced
GPU upload failure. The two Effect Games-derived fixture directories remain
local and untracked provenance inputs; no art assets were added to the tracked
delivery.

## Review and acceptance

- Reviewer / date: `simplysoup` / 2026-09-22
- Decision: accepted
- Unmet criteria / stale dependent evidence: none for S05; live manual-command and forced GPU upload-failure evidence remain outside this no-IPC increment and are not acceptance requirements for this stage
- Next owner / action: S06 remains planned pending its required time/astronomy policy decision and owner assignment
