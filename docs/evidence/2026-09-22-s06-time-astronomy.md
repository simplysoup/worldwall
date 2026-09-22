# Evidence — S06 civil time, astronomy, and seasons acceptance

- Date / operator: 2026-09-22 / Pi coding agent
- Reviewer / acceptance authority: `simplysoup` / 2026-09-22
- Revision or artifact hashes: working-tree source hashes are recorded in [the implementation update](../updates/2026-09-22-s06-time-astronomy.md); acceptance is recorded in the commit that adds this evidence
- Stage / requirements: S06; WW-06, WW-08
- Environment: Linux x86_64; Rust 1.94.0; Cargo 1.94.0; pure/headless test execution
- Inputs: fixed UTC timestamps, explicit latitude/longitude values, explicit UTC-offset schedules, injected solar events, and generated registry fixtures; no network, IP geolocation, telemetry, or bundled art
- Kind: pure unit, selector-boundary, and documentation validation

## Results

| Criterion | Exact command or test | Expected | Observed | Outcome | Artifact |
| --- | --- | --- | --- | --- | --- |
| S06-AC1 | `cargo test --locked --all-targets --all-features`; review ADR-0005 | Approved policy and deterministic vectors cover dawn/dusk, DST, leap day, hemispheres, polar states, and missing/invalid location | ADR-0005 was approved by `simplysoup`. Tests cover civil leap/epoch conversion, explicit DST transition offsets, dawn/dusk boundaries, polar day/night, northern/southern/equatorial seasons, missing inputs, invalid coordinates, and NOAA mid-latitude events | pass | `docs/decisions/ADR-0005-s06-time-astronomy-policy.md`; `src/environment.rs` |
| S06-AC2 | `cargo test --locked --all-targets --all-features`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; inspect `src/environment.rs` and `src/scheduler.rs` | Civil-time interpretation does not provide frame deadlines or corrupt monotonic animation scheduling | Environment snapshots consume UTC wall-clock seconds only; `FrameScheduler` remains integer monotonic-deadline based. Existing scheduler tests and all S06 tests pass | pass | `src/environment.rs`; `src/scheduler.rs` |
| S06-AC3 | `scene_registry::tests::environment_snapshot_changes_are_inert_until_an_explicit_trigger` under `cargo test --locked --all-targets --all-features` | Explicit refresh can use a changed snapshot; clock/boundary observation alone does not reselect | A startup request is made active, a changed snapshot with `ClockTick` is ignored without generation change, and the same snapshot applies on explicit `Reload` | pass | `src/scene_registry.rs` |
| S06-AC4 | Environment unit tests; `rg -n "IP geolocation|telemetry|println!|location" src/environment.rs` | Location is explicit, missing input is visible, and ordinary diagnostics do not expose precise coordinates | `Location` requires finite bounded coordinates, missing provider inputs produce unknown state, and the environment module emits no location-bearing diagnostics or network/geolocation behavior | pass | `src/environment.rs`; `docs/decisions/ADR-0005-s06-time-astronomy-policy.md` |

## Validation commands

| Check | Result |
| --- | --- |
| `cargo fmt --check` | pass |
| `cargo test --locked --all-targets --all-features` | pass — 49 passed, 2 ignored GPU tests |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | pass |
| `python3 tools/check_docs.py` | pass — 72 Markdown files, 12 stages, 7 skills |
| `python3 -m unittest discover -s tools/tests -v` | pass — 33 tests |
| `git diff --check` | pass |

## Limitations and interpretation

This acceptance covers the dependency-free S06 environment/snapshot and
selector-boundary contract. It does not claim a live compositor, hardware, or
power result. The provider is intentionally not network-aware and does not
infer location. Later runtime configuration/event-loop work must preserve the
accepted explicit-input and no-implicit-reselection rules; S07 owns weather
provider policy and S08 owns suspend/resume coalescing.

## Review and acceptance

`simplysoup` approved ADR-0005 and accepted S06 on 2026-09-22 after reviewing
the policy, implementation update, source tests, selector trigger boundary,
and validation results above. S06 is accepted; no S06 criterion is waived.

## Supersession

None.
