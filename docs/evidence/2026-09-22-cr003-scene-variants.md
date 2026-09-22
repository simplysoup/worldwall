# Evidence — CR-0003 complete scene-family and variant inventory

- Date / operator: 2026-09-22 / Pi coding agent
- Reviewer / approval: `simplysoup` / 2026-09-22 (implementation instruction)
- Revision or artifact hashes: no Git commit; `tools/generate_effectgames_fixtures.py` SHA-256 `61e7f873e675b5acbd240b1344859d7d969aac07976ab6d25319c43ba7223112`; `fixtures/effectgames-reference-manifest.toml` SHA-256 `abff1560a432a0130f7a7b449d5fe3abff8a7582de419006e19ee2fb95515de1`; live log SHA-256 `c7f0a6f88667e586ffb9001a08a269ff0c96b70dadbc663dbb5243def1c9f2c3`
- Change / stages / requirements: CR-0003; S04, S05, S06; WW-03, WW-04, WW-06, WW-08, WW-12; S04-AC1/S04-AC3, S05-AC1/S05-AC5, S06-AC3
- Environment: Linux x86_64; Rust 1.94.0; Cargo 1.94.0; Hyprland 0.56.2; Vulkan 1.4.357; NVIDIA GeForce RTX 3070 Ti; local 1920×1080 scale-1 outputs. The live run is observational local evidence, not assigned-host acceptance.
- Inputs: catalog snapshot from <https://www.effectgames.com/demos/worlds/scenes.js>; 21 browser-captured catalog-listed source responses; no network access by the Rust runtime, no weather/location input, and no tracked or published art assets
- Kind: metadata/provenance, parser/containment, pure package loading, and local live Wayland/Vulkan startup

## Results

| Criterion / scope | Exact command or procedure | Expected | Observed | Outcome | Artifact |
| --- | --- | --- | --- | --- | --- |
| Complete inventory | `python3 - <<'PY' ...` count `[[scene]]` and `[[scene.variant]]` in `fixtures/effectgames-reference-manifest.toml` | 12 scene families and 22 variants, with an explicit unavailable entry | 12 families, 22 variants, one `source-unavailable` V12 entry; duplicate catalog names are retained in `catalog_name` and separated by stable IDs | pass | `fixtures/effectgames-reference-manifest.toml` |
| S04-AC1 / S05-AC1 / source containment | `python3 tools/generate_effectgames_fixtures.py --check-only` | Every available source parses to a bounded 640×480 indexed image and 256-entry palette; generated package resources exist; unavailable source is not synthesized | 21 catalog-listed responses parsed; 21 package directories contain `scene.png`, `palette.png`, `scene.toml`, and provenance; V12 has no package and remains unavailable | pass for available sources; V12 explicitly not-run | `tools/generate_effectgames_fixtures.py`; local ignored fixture root |
| S04-AC3 / S06-AC3 | Generator output inspection plus package manifests | Source-specific remaps are applied during fixture derivation; optional timelines are contained resources and do not alter selection semantics | CORAL, V29, and October remaps are recorded in the canonical manifest and `SOURCE.txt`; source timelines become contained `time_of_day` resources; no renderer or selection trigger was changed | pass for derivation/boundary | canonical manifest; local package `scene.toml` files |
| S05-AC5 / complete package loading | `WORLDWALL_REGISTRY_DIR="$PWD/fixtures/untracked/effectgames" WORLDWALL_TEST_EXIT_AFTER_MS=1200 cargo run --locked` | Explicit registry validates all listed packages and starts one selected package through the existing renderer path | Registry loaded all 21 packages, selected `v25-desert--clear`, uploaded a 640×480 scene once, presented at 30 FPS, and shut down cleanly | pass for local implementation/demo; not assigned-session acceptance | `/home/soup/.local/state/worldwall/cr003-effectgames-20260922/registry-startup.log` |
| S04-AC2 / S04-AC3 / S05-AC5 / complete live sequence | For each generated package, `WORLDWALL_SCENE_DIR=<package> WORLDWALL_FPS=30 WORLDWALL_UTC_OFFSET_MINUTES=0 WORLDWALL_TEST_EXIT_AFTER_MS=10000 ./target/debug/worldwall` | Every available variant presents through the live Wayland/Vulkan path with one scene upload and clean owner-safe teardown; animation fixtures exercise bounded frame updates | All 21 variants completed 10-second runs: 300 presented frames (299 animation frames) except one 299/298 run, one scene upload, and clean teardown. 17 packages carried source-derived animation groups; four catalog sources were source-static and were not synthesized into animations. This run used the pre-correction source-rate mapping and is superseded for cycle-semantics claims by the correction below | pass for local observational sequence; not assigned-host acceptance | `/home/soup/.local/state/worldwall/cr003-effectgames-20260922/complete-live-sequence.log` (SHA-256 `9274d81e9451d4c0b1fddc776c3dbbcf7d9b498e8e0320a421280366ee1b8548`) |
| WW-06 / WW-08 selection boundary | Source review and existing selector tests under `cargo test --locked --all-targets --all-features` | Weather labels, civil-time timelines, and ordinary ticks do not silently reselect a scene | Existing S05/S06 trigger tests remained green; CR-0003 adds only metadata/packages and does not add a selector trigger | pass for preserved boundary | `src/scene_registry.rs` tests; `docs/evidence/2026-09-22-s06-time-astronomy.md` |

## Validation commands

| Check | Result |
| --- | --- |
| `python3 tools/generate_effectgames_fixtures.py --check-only` | pass — 21 catalog-listed variants parsed and generated packages found |
| `cargo test --locked --all-targets --all-features` | pass — 65 passed, 3 ignored GPU tests |
| `WORLDWALL_REGISTRY_DIR=fixtures/untracked/effectgames WORLDWALL_TEST_EXIT_AFTER_MS=1200 cargo run --locked` | pass — local Vulkan/Wayland startup, one upload, clean bounded shutdown; log retained outside Git |
| 21-package live sequence, 10 seconds/package at 30 FPS | pass — 17 source-derived animated packages and 4 source-static packages completed; 6,299 presented frames, 6,278 animation frames, one upload per package, clean shutdowns; log retained outside Git |
| `python3 tools/check_docs.py` | pass — 85 Markdown files, 12 stages, 7 skills |
| `python3 -m unittest discover -s tools/tests -v` | pass — 33 tests |

## Provenance and limitations

The generator uses browser-captured source responses and writes only under
`fixtures/untracked/effectgames/`. `.gitignore` now excludes that directory;
raw responses, derived indices/palettes, atlases, and packages are not part of
the tracked delivery. Catalog listing is not a license grant. No publication,
redistribution, weather request, location inference, telemetry, or session
configuration change was made.

The local registry startup validates and decodes every listed package because
the S05 registry loader fully loads each manifest during registry construction.
The complete 21-package sequence also ran locally through Wayland/Vulkan for
10 seconds per package at 30 FPS. Seventeen packages used source-derived
palette-cycle animation fixtures; four catalog sources had no active source
cycles and were intentionally left static rather than synthesized. This is
observational local evidence, not assigned-host acceptance or a claim of
visible pixel-difference correctness for every source effect. V12 is
`source-unavailable` because the catalog itself comments that its 640×480 LBM
is missing; no synthetic pass fixture was created.

## Review and application

- Decision: CR-0003 applied; no delivery-contract version or stage state changed.
- Accepted evidence affected: none; CR-0002's three-scene fixture evidence remains historical and is extended by this inventory.
- Next owner / action: integrator may submit this follow-up for review; assigned-host review remains separate from this local complete-matrix run.

## Cycle-semantics correction

The initial local complete-matrix logs above used an incorrect fixture mapping: an
arbitrary divisor treated `cycles[].rate` as a full-cycle speed and emitted
`palette_cycle`. Source inspection of `main.js` and `palette.js` confirmed that
the site computes `rate / floor(280 / speedAdjust)` as a palette-shift rate,
then the range length determines complete rotations; the site's default Blend
mode uses authored-byte BlendShift. The generator now uses the inspected
speed-adjust-4 formula `rate * 4 / 280 / range_length` and emits
`blend_shift` for the observed reverse modes 0/2. The old complete-matrix log
is retained as historical evidence but is stale for current cycle semantics.

Current checks and representative live validation:

- `python3 tools/generate_effectgames_fixtures.py --check-only` — pass for all
  21 available packages.
- V05 rain mapping for `rate=4914`, range length 5 — `14.040000` cycles/s,
  matching the site's speed-4 control rather than an arbitrary `0.4914` value.
- V05 rain, 3-second 30-FPS local Wayland/Vulkan run — one scene upload and
  clean owner-safe teardown; log `/home/soup/.local/state/worldwall/cr003-effectgames-20260922/live-js-mapped-v05-rain.log`, SHA-256 `24588e1beb8f911935a80e4b387d8037d2c348472031a829c422f4dcfdd124d3`.
- The complete matrix has not yet been rerun after this correction; no updated
  full-matrix or assigned-host acceptance claim is made.
