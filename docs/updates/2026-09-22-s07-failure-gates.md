# S07 weather failure-path and review-preparation increment

- Date / operator: 2026-09-22 / Pi coding agent
- Stage / requirements: S07; WW-06, WW-07, WW-11; S07-AC1–AC5
- Dependency / approval: S06 accepted; ADR-0006 and the bounded Open-Meteo live trace are recorded as approved; final S07 acceptance was recorded by `simplysoup` on 2026-09-22
- Kind: failure-path implementation, deterministic tests, local HTTP fault injection, and gate review preparation
- Source artifacts: `src/weather.rs` SHA-256 `c5c80f06ea9a264ea75cfd0ecaa939c85c6861ec875f382f69f19b4b8431c12d`; `src/wayland_surface.rs` SHA-256 `70f5181aa5dd085d77913b702530d520ee120562adf91e511cc233bf92cd81d6`

## Outcome

Completed the remaining bounded S07 failure-path slice without changing the
renderer or selector ownership boundaries:

- Open-Meteo HTTP status classification is exercised through a local HTTP
  fixture for rate-limit errors, and the finite client timeout is exercised by
  a delayed local response. No external request is made by the test suite.
- A cancellable slow fake provider verifies that an offline/deadline failure
  cancels the worker request, retains one in-flight ownership until the worker
  acknowledges cancellation, and cannot start an overlapping retry.
- Best-effort cache attachment retains a valid in-memory reading when the disk
  record is corrupt. With no valid memory value, a corrupt cache produces an
  explicit error/unknown fallback and the next valid provider result atomically
  replaces the corrupt file.
- Duplicate `RefreshKind::Resume` requests coalesce through the existing
  one-in-flight gate; the test observes one provider call and one cache epoch.
  Weather completion still has no selector handle and cannot advance scene
  selection. Real suspend/resume signals remain S08 work.
- Runtime cache startup now keeps the cache store installed after rejection so
  recovery can replace the bad record, and emits only a provider-neutral error
  state/fallback trace.

## Results

| Criterion | Exact command or test | Expected | Observed | Outcome | Artifact |
| --- | --- | --- | --- | --- | --- |
| S07-AC1 | `weather::tests::disabled_weather_makes_zero_provider_requests`, `cadence_and_one_in_flight_are_enforced`, `duplicate_resume_signals_coalesce_to_one_provider_request` | Disabled mode makes no requests; normal cadence and one in-flight request are bounded; duplicate resume work coalesces | Passed; duplicate resume signals produced one provider call and one cache epoch | pass for deterministic worker policy | `src/weather.rs` |
| S07-AC2 | `weather::tests::open_meteo_current_response_maps_documented_wmo_values`, `open_meteo_maps_http_errors_and_request_timeout`, cache round-trip/corruption tests | Provider errors, normalization, version checks, and atomic cache recovery are bounded | Passed; malformed/corrupt data was rejected, valid memory was retained, and a later valid result replaced the bad file | pass for pure/local fault coverage | `src/weather.rs` |
| S07-AC3 | `weather::tests::offline_request_timeout_cancels_without_overlapping_a_retry`; authorized live trace in [S07 live check](2026-09-22-s07-live.md) | Slow/offline work does not block rendering and uses a finite fallback path | Passed in worker/local fault tests and the assigned live success trace; no separate offline compositor run was made | pass for recorded test layers; live offline fault remains not-run | `src/weather.rs`, live artifact |
| S07-AC4 | `weather::tests::duplicate_resume_signals_coalesce_to_one_provider_request`, `late_completion_updates_only_cache_epoch_not_selection` | Poll/completion changes cache only and does not reselect | Passed; cache epoch changed without a selector dependency or scene-generation mutation | pass for implementation boundary | `src/weather.rs`, `src/scene_registry.rs` |
| S07-AC5 | Source review; `python3 tools/check_docs.py` | Privacy/configuration behavior remains explicit and ordinary diagnostics redact location data | Passed; tests use local fixture coordinates only, runtime traces expose provider/state, and the approved ADR/live record retains no coordinates | pass for documented behavior | `README.md`, `docs/decisions/ADR-0006-s07-open-meteo.md` |

## Validation

- `cargo fmt --check`: pass.
- `cargo test --locked --all-targets --all-features`: pass — 62 passed, 2 ignored GPU tests.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: pass.
- `cargo build --locked`: pass.
- `python3 tools/check_docs.py`: pass — 79 Markdown files, 12 stages, 7 skills; archive intact.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/tests -v`: pass — 33 tests.
- The local HTTP fixtures are not Open-Meteo and do not establish universal
  network or provider compatibility. The existing authorized live trace remains
  the only configured provider/compositor observation.

## Gate decision

`simplysoup` accepted S07 on 2026-09-22 after review of S07-AC1–AC5, the
ADR-0006 terms/privacy record, the authorized live trace, and this
failure-path evidence. The acceptance covers the bounded optional weather/cache
path and does not claim real suspend/resume behavior or power-event coalescing;
those remain S08 work.

## Next action

S08 is now the next planned stage. No suspend/resume hardware claim is made;
real power-event coalescing belongs to S08.
