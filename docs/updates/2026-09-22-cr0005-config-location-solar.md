# CR-0005 — configured location, optional weather, and solar palette implementation

**Date:** 2026-09-22
**Change / requirements:** CR-0005; S04, S06, S07, S10; WW-03/04/06/07/08/10/12
**Owner:** Pi coding agent
**State:** implementation applied; CR-0005 add-on acceptance recorded; assigned-session visual/pacing evidence remains incomplete; not a new stage acceptance

## Bounded outcome

Applied approved CR-0005 in the working tree. Config schema v2 accepts an optional all-or-nothing explicit `[location]` with bounded coordinates and a recognized IANA timezone, plus optional weather defaulting to disabled. Schema v1 remains readable, and legacy environment inputs are used only when `WORLDWALL_CONFIG` is unset. Config reload prepares registry, timezone, provider, cache scope, and scheduling changes before commit; it does not reselect scenes.

The environment layer uses the pinned timezone database for local civil time/DST and resolves NOAA sunrise/sunset events for previous/current/next local dates. Optional `time_of_day.toml` v2 solar timelines resolve to a bounded row-pair/fraction; the immutable atlas is sampled by WGSL. Missing/polar/unavailable/duplicate/unbracketed samples fall back to the base palette. Civil schema-v1 behavior remains intact. Weather cache schema v2 carries a config-metadata generation scope rather than raw location data.

At the time of initial implementation, no operator location or timezone had been supplied, so no local config values had been added and weather remained disabled. No Open-Meteo request was made. A pre-existing Worldwall process was not stopped or restarted. The V29 provenance fixture was not edited; its speed-adjusted private copy is recorded below after validation. The later explicit user config is recorded in the follow-up section.

## Changed files

- `Cargo.toml`, `Cargo.lock` — pinned `chrono`, `chrono-tz`, and `toml` dependencies.
- `src/config.rs` — schema v1/v2 parsing, explicit location/timezone/provider, authoritative file semantics, and metadata-derived cache scope.
- `src/environment.rs` — IANA/DST conversion and local-date solar-event support across UTC midnight.
- `src/scene.rs` — bounded solar timeline v2 parser, keyframe validation, anchor resolution, and fallback.
- `src/renderer.rs` — bounded uniform sample and WGSL atlas interpolation; existing immutable upload path retained.
- `src/weather.rs`, `src/wayland_surface.rs` — scoped cache, config-derived services, and transactional runtime application.
- `docs/delivery-contract.md`, `docs/scene-format.md`, `docs/ipc.md`, `docs/release.md`, `docs/architecture.md`, `README.md`, examples, affected stage/ADR records, status, roadmap, CR register, and this update — reconciled to CR-0005.

## Validation

| Check | Result |
| --- | --- |
| `cargo fmt --check` | pass |
| `cargo test --locked --all-features` | pass — 92 main tests and 4 CLI tests passed; 3 ignored |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | pass |
| `cargo test --locked --all-features renderer::tests::gpu_time_of_day_palette_readback_matches_byte_floor_reference -- --ignored --nocapture` | pass — 1 offscreen Vulkan readback test, including solar row-pair/fraction and base fallback |
| `python3 tools/check_docs.py` | pass — 108 Markdown files, 12 stages, 7 skills; archive intact |
| `python3 -m unittest discover -s tools/tests -v` | pass — 41 tests |
| `git diff --check` | pass |

## Gate status and limitations

- Automated tests cover schema-v1/v2 parsing, IANA/DST conversion, UTC-midnight solar events, adjacent-date interpolation/wrap, duplicate/polar fallback, cache-scope mismatch, disabled-weather zero requests, candidate rollback, and shader row-pair/fraction/base readback.
- **S04-AC6:** partial evidence only; explicit daily seasonal movement vectors and the full malformed/unknown matrix still need review.
- **S04-AC7:** offscreen GPU readback passes. The later assigned-session trace records stable one-time scene upload; output 114 pacing follows the requested intervals, while output 115 is sparse and requires review.
- **S06-AC5:** pure IANA/DST and UTC-midnight astronomy tests pass. Fake-provider and assigned-session config-reload traces show no scene reselection and unchanged selection generation.
- **S07-AC6/S10-AC6:** parser/cache/provider-replacement/candidate-rollback tests pass; the later session run exercised valid reloads and invalid-config rollback with weather disabled. No Open-Meteo request was made.
- **S04-AC8:** partial assigned-session evidence exists for the synthetic authored solar atlas, solar sample trace, steady upload count, and output-specific frame intervals. The user reported that this temporary synthetic palette looked purple/all wrong; it is recorded as a visual `fail` for that fixture, not as evidence against the source V29 palette. Output 115's sparse callback/frame series remains for review. Future V29 live fixtures preserve the source civil-time atlas. Headless readback and screenshots alone do not satisfy the visual gate.
- No live weather-provider test or Open-Meteo request was performed during implementation validation. The later follow-up explicitly enables Open-Meteo for the user's private config; no request was made while applying it.
- At the time of the initial implementation update, no new binary was started in the existing Hyprland session. The pre-existing process was left untouched and the edited Lua callback was staged, not active. A later, separately authorized assigned-session run used only its own child and is recorded in the 2026-09-23 test-batch/live-run guide.
- `vulkaninfo --summary` reported Vulkan 1.4.357, NVIDIA GeForce RTX 3070 Ti, driver 615.71.09. The ignored readback ran against Vulkan on this host; this is GPU shader evidence only, not visible Wayland presentation or assigned-session acceptance.

## Follow-up: approved private operator config

On 2026-09-22, `simplysoup` reconfirmed approval of the CR-0005 add-on and supplied explicit location and IANA timezone inputs with weather enabled. The private config (mode `0600`) uses the supplied location and timezone, `[weather].provider = "open-meteo"`, and retains 30 FPS. Precise coordinates are intentionally not duplicated in project documentation. The tracked example remains location-free and weather-disabled.

`cargo build --locked --release` passed; the binary was installed privately with SHA-256 `942dadda0b54e528198ad13c4e8db225bdd7ebdf2665a0921f7c6479f6358535`. A Python `tomllib`/`Decimal`/`ZoneInfo` check verified the private config's schema, exact supplied values, canonical timezone, enabled provider, FPS, and file permissions without printing the coordinates. No application was started, no config reload was sent, and no Open-Meteo request was made. The staged Hyprland Lua callback now passes `WORLDWALL_CONFIG` and the private V29 scene directory to this current binary. It is not active until the staged configuration is selected at a future compositor startup; the pre-existing process remains untouched. At next startup the runtime will attempt the enabled bounded asynchronous refresh subject to its cache/cadence rules; no request was sent during this config update.

The user approval authorizes the add-on/config but does not pass outstanding S04/S06/S07/S10 technical gates or substitute for assigned-session solar visual/pacing/upload evidence.

## Private V29 demonstration copy

The provenance fixture remains unchanged at `fixtures/untracked/effectgames/v29-seascape`; its `animation.toml` SHA-256 is `258081dfc76076519f64b56be9b59b7d95f8621fa988cf174d88eca9543bc0f4`. The private V29 copy has the requested 0.5× BlendShift speeds: `0.547768`, `0.754018` (three groups), `0.8223215`, and `1.097143`. A `tomllib`/`Decimal` check verified all seven exact values, each equals half its source value, all other animation fields match, and every other copied file is byte-identical. The private `animation.toml` SHA-256 is `68124ed350c5d9db6cb6e54308f6dd5f06d79e7ecf602ca912631944dd54b5f7`. After that validation, the staged Hyprland Lua callback was first updated to use this private directory while retaining 30 FPS. The follow-up below supersedes its binary/config arguments; the callback remains staged, not active, and the running application was not restarted.

## Artifact identity and next action

No Git commit is present. The source tree baseline observed during this increment was `c5263e746af1b3ee63e246a1cdd2dbea202ec7cf`, with the implementation in the working tree. Keep the following distinct: source/unit results, offscreen GPU readback, private fixture validation, and assigned-session evidence.

## Follow-up — 2026-09-23 test batch and acceptance disposition

`simplysoup` explicitly clarified on 2026-09-22 that approval marks the CR-0005 add-on gates accepted. This is recorded in [CR-0005](../changes/CR-0005-configured-location-weather-solar-lighting.md) as maintainer acceptance, separate from technical test outcomes. The 2026-09-23 offline batch is recorded in the [run guide and evidence record](2026-09-23-cr0005-test-batch-run-guide.md): 95 main Rust tests plus 4 CLI tests passed (3 GPU-only tests remained ignored in the general run), explicit solar Vulkan readback passed, Clippy/format/build/docs/tool tests/diff checks passed, and fake-provider config-reload/cancellation coverage passed. A follow-up Python tooling run passed 49 tests after a live-runner metrics enhancement.

The first live preflight found a pre-existing process and recorded `launched=false`; the agent did not stop/restart it. A separate runner attempt then hit `TextIOWrapper.chmod` before launching a child; the path-based permissions fix has regression coverage and its private result is documented under [FM-0014](../knowledge/failures/FM-0014-python-log-stream-permissions.md). After the user made an approved session window available, the corrected runner completed its own 90-second child. It did not stop or replace a pre-existing process.

The assigned-session log records `scene-source=package`, two ready outputs, solar sample rows 3→0, weather disabled, stable `shared-scene-uploads=1`, valid 30→15→30 FPS reloads without reselection, and invalid-config rollback with unchanged selection generation. Output 114's frame intervals track the requested FPS; output 115 has sparse observations (27 at 30 FPS and 6 at 15 FPS) with long intervals. No pacing threshold was configured, and output 115 requires review before claiming steady multi-output pacing. The user-reported visual result is `fail` for the synthetic atlas; the user later reported the source-preserving V29 display looked good in an interrupted run whose technical results were not finalized. The IPC `scene` label `synthetic-reference` is not relied upon for identity. The purple report does not judge the original V29 palette, whose time-of-day files are byte-identical to the source fixture. The private solar-v2 test fixture and weather-disabled config remain in the evidence bundle; the source V29 fixture is unchanged. No Open-Meteo request was made.
