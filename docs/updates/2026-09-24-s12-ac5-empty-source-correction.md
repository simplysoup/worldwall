# S12 corrections — fail-closed empty sources and curator defaults

- **Date / operator:** 2026-09-24 · Pi coding agent
- **Base revision:** `df195af` (`main`); working tree already contained the S12 implementation
- **Stage / requirements:** S12-AC3/AC5 · WW-03, WW-06, WW-08
- **Kind:** Rust and JavaScript unit/regression tests
- **Decision:** No scope or contract change. The 2026-09-23 S12 acceptance remains historical; S12 is returned to review pending maintainer review of the AC5 correction and the related curator fixes.

## Finding

Review of startup source preparation found a configuration not covered by the
accepted AC5 tests: schema v5 can reference a valid sidecar containing a pool
with an empty `scene_ids` list while `registry_dir = "none"`. `prepare_sources`
then returned `(None, Some(pools))`; the startup loader treated `None` as
legacy mode and loaded the synthetic reference scene without consulting the
configured eligibility gate. This contradicted the fail-closed no-candidate
contract.

A regression test first reproduced the issue on the pre-fix working tree:

```sh
cargo test --offline --locked empty_pool_sidecar_without_registry_fails_closed_instead_of_defaulting_scene -- --nocapture
```

Observed before the fix: failed at the assertion requiring pool mode to
materialize an empty registry (`pool mode must materialize an empty registry
rather than permit the synthetic scene`).

The same review also found that the curator's first-save defaults assigned
`source = "library"` to every pool, including configured-registry scenes. The
helper grouped only by collection name, which could generate a sidecar that
runtime validation rejected. The UI code review additionally identified
WebGL's default four-byte row alignment for indexed uploads and a stale async
preview completion that could overwrite a more recent scene choice.

## Correction

`AppConfig::prepare_sources` now always returns a `SceneRegistry` when a pool
sidecar is configured, using an empty registry if no registry or pool package
was loaded. Startup consequently reaches the shared pool selector, which
returns `NoScene("no eligible scene")`; it cannot reach the legacy synthetic
reference-scene path. Configurations without a sidecar retain their prior
behavior. The regression test verifies the empty source and the no-eligible
startup selection result.

The curator's missing-sidecar defaults now use the tested pure helper in
`src/bin/curator_pools.js`, grouping by both source and collection and generating
safe unique pool IDs; registry scenes remain in registry pools. The WebGL
preview sets `UNPACK_ALIGNMENT = 1` for arbitrary-width indexed images, ignores
stale scene-load completions, begins on the first catalog scene, and keeps only
downsampled thumbnail pixels plus the currently selected preview assets.

A follow-up config-review case found that removing the only configured pool
sidecar when no registry was configured could leave an empty selector installed.
The config transaction now clears the selection source when the validated
replacement has no registry or sidecar, restoring legacy mode without selecting
or replacing the resident renderer scene; a regression test covers this reverse
transition.

A separate validator inconsistency found during review was corrected: the
runtime `display_name` validator now counts Unicode characters and enforces the
same 200-character bound as the Python catalog validator (with a boundary test).
The tools README was also refreshed to reflect the already-recorded FM-0016
harness repair rather than its stale pre-repair state.

## Results

| Criterion | Exact command or procedure | Expected | Observed | Outcome | Artifact |
| --- | --- | --- | --- | --- | --- |
| S12-AC5 empty-source fail-closed regression | Baseline command above, before and after the fix | Fails before correction; passes after it | Baseline failed at the missing-empty-registry assertion; post-fix targeted test passed (1 passed) | pass | `src/config.rs::tests::empty_pool_sidecar_without_registry_fails_closed_instead_of_defaulting_scene` |
| S12-AC3 source-aware default pools | `python3 -m unittest tools.tests.test_curator_pool_defaults -v` | Library and registry scenes are grouped into source-correct pools with unique valid IDs | One Node.js-backed test passed; mixed sources and sanitized-name collisions were covered | pass | `src/bin/curator_pools.js`; `tools/tests/test_curator_pool_defaults.py` |
| S12-AC5 sidecar-removal transition | `cargo test --offline --locked --all-targets --all-features removing_pool_sidecar_without_registry_clears_selection_source` | Removing the last configured source clears pool-mode selection without leaving a phantom empty registry | Targeted regression test passed (1 passed) | pass | `src/wayland_surface.rs::tests::removing_pool_sidecar_without_registry_clears_selection_source` |
| S12-AC3/AC5 broader revalidation | `cargo fmt --all --check`; `cargo clippy --offline --locked --all-targets --all-features -- -D warnings`; `cargo test --offline --locked --all-targets --all-features`; `cargo build --offline --locked --all-targets --all-features`; `python3 tools/check_docs.py`; `python3 -m unittest discover -s tools/tests -v`; `node --check src/bin/curator.js`; `node --check src/bin/curator_palette.js`; `node --check src/bin/curator_pools.js`; `bash -n tools/run_s12_pool_smoke.sh`; `bash -n tools/run_nested_s01.sh`; `git diff --check` | All applicable offline checks pass | Formatting, Clippy, and locked build passed; Rust tests: 132 application + 78 CLI passed, 3 Vulkan tests ignored; docs checker passed (138 Markdown files, 13 stages, 7 skills); 101 Python tests passed; Node syntax, shell syntax, and diff checks passed | pass | This record and test output from the 2026-09-24 validation run |

## Limitations and failures

- The Rust and JavaScript tests validate source preparation, the shared
  selector result, sidecar-removal behavior, and source-aware default-pool
  construction; they are not a new live curator or compositor run. The added WebGL alignment and UI stale-
  completion guards were syntax-checked but not exercised in a browser here.
- The assigned live Worldwall session was not restarted or reconfigured.
- The full S12-AC3/AC5 gates have not been resubmitted for acceptance; the
  existing AC3 host and AC4/AC5 live-smoke evidence remains historical and is
  not silently rewritten.
- Artwork license state remains `unreviewed`; no assets were published or
  distributed.

## Review and next action

- **Reviewer / date:** not reviewed; assigned maintainer acceptance remains
  required.
- **Decision:** corrections implemented and broader offline validation passed;
  maintainer review pending.
- **Unmet criteria / stale evidence:** S12-AC3/AC5 maintainer re-review; the
  historical live smoke record does not specifically cover an empty sidecar
  with no registry and empty pool memberships, and the curator default-pool
  helper has no recorded live browser exercise.
- **Next owner / action:** `simplysoup` to review the correction and decide
  whether to restore S12 to `accepted`; the requested repository commit/push
  does not itself constitute stage acceptance or a release claim.
