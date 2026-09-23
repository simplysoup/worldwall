# S12-AC2 increment — versioned catalog and pool/rules sidecar model

**Date:** 2026-09-23
**Stage / requirements / gate:** S12; WW-03, WW-04, WW-06, WW-07, WW-08, WW-10, WW-12; S12-AC2 (accepted by `simplysoup`, 2026-09-23; see below)
**Owner:** Pi coding agent
**Acceptance authority:** `simplysoup`, per contract v0.8
**Baseline revision:** `1490eff` ("S12: accept AC1 and add private capture importer"); the checkout was already dirty with unrelated changes and those files were preserved.
**Outcome:** Define the final versioned scene-pool library catalog and pool/scene-rules sidecar models against the approved AC1 policy, with a bounded, path-safe offline validator/builder and failure-path tests. With the earlier importer slice this completes the S12-AC2 model work; `simplysoup` accepted S12-AC2 on 2026-09-23 (recorded below). No runtime, registry, config, session, or Worldwall-process behavior changed.

## Model definition

- [docs/scene-pools.md](../scene-pools.md) defines `library.toml` schema v1 (explicit catalog of complete packages: scene IDs, package paths, complete image/palette/animation/time-of-day/source-cycle assets with SHA-256 digests, `world`/`appearance` relationships with explicit `unknown`, source provenance/hash, and `license_state`) and `scene-pools.toml` schema v1 (pools with `id`/`source` (`library` or `registry`)/`enabled`/`scene_ids`, and one editable `[[scene_rule]]` per scene with `enabled`, `time_of_day`, `temperature_min_c`/`temperature_max_c`, and `weather_any`).
- The model follows the accepted AC1 policy: pools name complete validated package IDs and duplicate membership across pools/sources is rejected; configured criteria use the environment vocabulary minus `unknown` and require known inputs; the eligibility/no-candidate/manual-override semantics and the unchanged trigger matrix are recorded normatively in the document for the future S12-AC5 filter. `scene-format.md`'s pool-eligibility section now links it; CR-0006 adaptive-registry metadata is explicitly outside the catalog and unchanged.
- Catalog values that are not established by evidence (appearance relationships, license grants) stay explicit `unknown`/absent and are never inferred from source titles. License states (`unreviewed`, `reviewed-granted`, `reviewed-withheld`, with `license_id` required only for `reviewed-granted`) and `cycle_semantics = "unmapped-source-native"` are recorded as declared and never upgraded.

## Implementation

- Added `tools/scene_pool_library.py` (Python 3.11+ standard library only, `tomllib` for data-only parsing). `--check-only` validates an existing catalog (and optionally a sidecar) and writes nothing; `--write-catalog` rebuilds `library.toml` from complete packages only after full validation and replaces it atomically (mode 0600, fsync, rename) so failures preserve the prior file. It reads only explicitly supplied local paths, fetches nothing, and executes no source code.
- Bounds and safety: 16 MiB per file, 256 packages, 32 files per package, 16 Mi-pixel scene images, 256×1 8-bit RGBA palettes, 256 source cycles/256 animation groups/256 keyframes; relative path references without `.`/`..`/backslashes, symlink-free components, regular non-symlink files opened with `O_NOFOLLOW`; strict key sets everywhere with duplicate/unknown-key rejection.
- Validation depth: scene/provenance manifest consistency with the catalog and between each other; PNG header checks matching the accepted S03 decoding contract (8-bit grayscale/indexed scene, 256×1 opaque RGBA palette); strict source-cycle validation (bounded integer fields, at least two indices per range, non-overlapping ranges, `unknown` semantics) matching the importer rules; structural-only validation for `animation.toml`/`time_of_day.toml` (parseable, known `schema_version`, non-empty payload) because full effect semantics remain the accepted runtime loader's gate.
- Sidecar validation rejects duplicate pool IDs, duplicate scene membership across pools (including across `library`/`registry` sources), duplicate/unknown rules, rules for unpooled scenes, malformed constraints (min > max, non-finite or out-of-range temperatures, empty/duplicate/unknown `time_of_day`/`weather_any` values), and library-pool scenes missing from the catalog. Registry-sourced membership is left to the runtime resolver.
- `--write-catalog` preserves curator-authored `world`/`appearance` relationships across regeneration; `source_collection` is derived from the validated `source_format` identifier by a fixed mapping, never from titles or content.
- Added `tools/tests/test_scene_pool_library.py` (30 tests) covering catalog round-trip with provenance, relation preservation, non-copying of `family`/`temperature_band`, path escapes and symlinks, file-set/digest/image failures, duplicate IDs, manifest disagreement, animation/source-cycle exclusivity and invalid cycle/animation data, license-state rules, oversize files, sidecar duplicates and malformed rules, and no-partial-state CLI behavior. Updated [tools/README.md](../../tools/README.md).

## Validation

- `python3 -m unittest tools.tests.test_scene_pool_library -v` — **pass**, 30 tests.
- `python3 tools/scene_pool_library.py --library fixtures/untracked/effectgames/canvascycle-v25heat --write-catalog` — **pass**; 1 catalog scene, 5 package files, 16 source cycles validated. The generated `library.toml` records `world`/`appearance` as `unknown`, `license_state = "unreviewed"`, and the previously recorded source hash `2d0ee7fa2d83ab986514e76db05a88f6735a6186dde28fb6466a6fd45ed2305f`; `git check-ignore` confirms it is ignored private material.
- `python3 tools/scene_pool_library.py --library fixtures/untracked/effectgames/canvascycle-v25heat --check-only` and the same run with a temporary `scene-pools.toml` (one library pool plus one rule with time/temperature/weather criteria) — **pass** (`pools=1 rules=1`); the temporary sidecar was deleted afterward.
- `python3 tools/check_docs.py` — **pass**, 125 Markdown files, 13 stages, 7 skills.
- `python3 -m unittest discover -s tools/tests -v` — **pass**, 96 tests (66 pre-existing + 30 new).
- `python3 -m py_compile tools/scene_pool_library.py tools/tests/test_scene_pool_library.py` and `git diff --check` — **pass**.
- No Rust/runtime code, adaptive registry, active config, Firefox, or Worldwall process was changed or controlled in this increment.

## Formal S12-AC2 acceptance — 2026-09-23

`simplysoup` accepted S12-AC2 on 2026-09-23 after reviewing the catalog/sidecar model, the bounded importer/validator, and the recorded offline evidence in this record and the [AC2 importer record](2026-09-23-s12-ac2-local-capture-importer.md). This accepts S12-AC2 only; it does not accept S12 or any later gate, and it does not establish a CanvasCycle license or authorize publication of artwork.

## Limitations and next action

With S12-AC2 accepted above, the remaining S12 gates stay open: `worldwallctl curator` (AC3), schema-v5 runtime consumption and transactions (AC4), the shared automatic-selection filter (AC5), GPU preview fidelity (AC6), and host/privacy/release evidence (AC7) are not implemented. `animation.toml`/`time_of_day.toml` are validated structurally here, not by full runtime semantics. Only the one locally captured V25HEAT package exists; the other CanvasCycle entries remain unavailable and must not be fetched or synthesized, its source-native cycles remain unmapped, and no capture carries a reviewed license. XDG default locations remain an AC3 decision. This increment is offline tooling evidence only; no compositor, GPU, or hardware claim is made.

Next: continue with the bounded **S12-AC3** curator command/UI slice using the approved external Firefox host.
