# RB-0005 bounded loading and private crop validation

- Date / operator: 2026-09-23 · Pi coding agent; no live operator session
- Revision: `c5263e7` base; working tree remains uncommitted
- Stage / requirements: CR-0006 supplemental S05-AC4 / S10-AC8 async path; S05-AC7 and assigned-session acceptance remain open
- Environment: offline Rust 1.94.0 / Cargo 1.94.0; no Wayland, GPU, weather, or network session was started
- Kind: implementation, deterministic unit tests, private application-loader validation, documentation

## Results

| Criterion | Exact command or procedure | Expected | Observed | Outcome | Artifact |
| --- | --- | --- | --- | --- | --- |
| Scene-worker responsiveness and bounds | `cargo test --offline --all-targets --all-features scene_registry::tests::scene_load_worker_queue_is_bounded_and_decode_errors_are_reported` | A calloop timer dispatches while a fake decode is blocked; one queued job is accepted, overflow is rejected, and failures return as completions. | Passed; the test also confirms the scene completion wakeup reaches calloop. | pass | `src/scene_registry.rs` test |
| Config-worker responsiveness and bounds | `cargo test --offline --all-targets --all-features wayland_surface::tests::config_worker_is_bounded_and_event_loop_dispatches_during_slow_validation` | An unrelated owner timer dispatches while fake config validation is blocked; queue overflow and validation error are bounded. | Passed. | pass | `src/wayland_surface.rs` test |
| Candidate rollback and stale generations | `cargo test --offline --all-targets --all-features` | Invalid prepared config leaves config/scene state unchanged; failed or invalidated candidates cannot replace resident resources. | Existing config transaction, scene transaction, stale-completion, and renderer-swap failure tests passed in the full suite. | pass | Rust test output below |
| Full offline Rust checks | `cargo fmt --all --check`; `cargo clippy --offline --all-targets --all-features -- -D warnings`; `cargo test --offline --all-targets --all-features`; `cargo build --offline --locked --all-targets --all-features` | Formatting, warning-free lint, all tests, and locked offline build pass. | Formatting/clippy/build passed; 111 main tests and 4 CLI tests passed; 3 Vulkan readback tests remained ignored as hardware-dependent. | pass | Local command output |
| Documentation/tool checks | `python3 tools/check_docs.py`; `python3 -m unittest discover -s tools/tests -v`; `git diff --check` | Documentation links/ledger and tool regressions pass; diff is whitespace-clean. | Docs checker passed for 118 Markdown files; all 49 tool tests passed; diff check passed. | pass | Local command output |
| Private crop application-loader validation | Temporary local Rust harness called `AppConfig::load_prepared` on the active private config; command: `cargo run --offline --quiet --bin validate_private_registry`. The helper source was removed after validation. | The active private config returns the validated one-scene registry and decodes the cropped resource. | Passed: `vw3b-monolith-plains--clear`, 328×446, four animation groups. The first attempt rejected `family_id`; only the private activation copy was corrected to the parser's `family` key, then validation passed. | pass | Private one-scene registry outside the repository |
| Private crop activation | Backed up the private config, set only `registry_dir` to the validated one-scene registry, and verified the resulting config through `AppConfig::load_prepared`. | Make the approved crop the sole ordinary registry scene without enabling adaptive selection or starting Worldwall. | Private schema-v2 config now points to the registry; adaptive selection remains off. The review guide, source fixture, and crop pixels are unchanged. | pass | Private config backup retained outside the repository |

The implementation uses bounded startup loader threads, a bounded config worker returning a `PreparedAppConfig` with its already-validated registry, and separate bounded scene workers for explicit IPC and adaptive work. Calloop wakeups do not depend on frame timers. The Wayland owner continues dispatch/render/power/weather-cache work, serializes IPC commands FIFO, and retains final success/error response semantics and the existing timeout; uploads and transaction commits remain on the owner. Generation invalidation and the existing transaction boundary are unchanged.

## Limitations and failures

- The first private loader attempt failed because the copied manifest used unknown key `family_id`; the activation copy now uses the application schema key `family`. The review package and source artwork/guide were not modified.
- No Worldwall process was launched/restarted. No IPC command, weather/provider request, adaptive activation/swap, assigned-session test, visual-stall measurement, or next-login test was performed.
- The private registry activates the crop only as an ordinary one-scene fallback. Adaptive selection remains disabled. Loader success and user visual approval do not satisfy S05-AC7, CR-0006 acceptance, or live compositor gates.
- Client-facing final response semantics remain unchanged; a load exceeding the existing two-second command timeout can still outlast the client wait, as before.

## Review and acceptance

- Reviewer / date: not submitted
- Decision: implementation and deterministic tests recorded; RB-0005 technical blocker resolved, not stage acceptance
- Unmet criteria / stale evidence: assigned-session S05-AC7 and separate CR-0006/S07/S08 supplemental gates remain open; historical S00–S11 acceptances are unchanged
- Next owner / action: `simplysoup` review the bounded worker/FIFO behavior; obtain separate authorization before any live or next-login validation
