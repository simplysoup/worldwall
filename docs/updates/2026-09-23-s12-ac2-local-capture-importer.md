# S12-AC2 increment — bounded local CanvasCycle capture import

**Date:** 2026-09-23
**Stage / requirements / gate:** S12; WW-03, WW-04, WW-10, WW-12; S12-AC2 (open)
**Owner:** Pi coding agent
**Acceptance authority:** `simplysoup`, per contract v0.8
**Baseline revision:** `64d558477ee5363c95d1713cbf7e863a69936e46`; the checkout was already dirty and unrelated changes were preserved.
**Outcome:** Implement a local-only, bounded parser/materializer slice for an explicitly supplied CanvasCycle LBM capture. This does not complete AC2 or define the final library/catalog and runtime sidecar schemas.

## Local source audit

- In the inspected repository fixture tree and `$HOME/.local/state/worldwall`, the only CanvasCycle source capture found was `fixtures/untracked/effectgames/canvascycle-v25heat/V25HEAT.LBM.json` (975,170 bytes). Its recorded SHA-256 is `2d0ee7fa2d83ab986514e76db05a88f6735a6186dde28fb6466a6fd45ed2305f`; `SOURCE.txt` records the CanvasCycle asset/page and says no independent license grant is established.
- The other `v08-jungle-waterfall-*` captures in the ignored fixture tree are Effect Games world-package sources, not CanvasCycle entries. The V08 CanvasCycle time variants noted by CR-0008 were not present in the inspected local roots. No source was fetched, and no missing CanvasCycle entry was synthesized.
- The available V25HEAT object reports 640×480, 256 RGB colors, 307,200 index bytes, and 16 non-overlapping cycle ranges. The local capture contains raw `reverse`, `rate`, `low`, and `high` values, but no captured engine code establishing rate/direction semantics. Those values are retained unchanged and are explicitly marked unmapped.

## Implementation

- Added `tools/import_canvascycle_capture.py`, which reads an explicit regular non-symlink file with a 16 MiB cap and parses a strict literal-data subset without evaluating JavaScript. It bounds pixels to the existing 16 Mi-pixel scene limit, requires exactly 256 RGB entries, limits cycle entries to 256, validates byte/range bounds and overlap, and rejects duplicate/unknown fields, expressions, malformed values, and trailing code.
- `--check-only` validates and reports structural counts/hash without writing. Package materialization requires explicit ID/provenance and a new output path. It writes grayscale indexed `scene.png`, opaque RGBA `palette.png`, a schema-v1 scene manifest, provenance with `license_state = "unreviewed"`, and `source-cycles.toml` retaining the raw cycle records. HTTPS provenance URLs and UTF-8 title text are validated before TOML output. The package directory is mode 0700 and files are mode 0600; existing output is refused.
- No `animation.toml` is generated because source cycle rate/direction semantics have not been established. No time/weather/temperature criteria are inferred from the entry title. The raw capture stays in its ignored local path. After the temporary validation package was removed, the one available capture was materialized for local inspection at `fixtures/untracked/effectgames/canvascycle-v25heat/worldwall-scene-package`; this generated package is ignored/private and must not be staged or published.
- Added `tools/tests/test_canvascycle_import.py` for literal-only parsing, bounds, dimensions, malformed/overlapping cycles, symlink/oversize input, no-overwrite behavior, PNG round-trip, provenance and a conditional check of the private V25HEAT capture. Added tool documentation in `tools/README.md`.

## Validation

- `python3 -m unittest tools.tests.test_canvascycle_import -v` — **pass**, 11 tests, including the locally available V25HEAT capture.
- `python3 tools/import_canvascycle_capture.py --input fixtures/untracked/effectgames/canvascycle-v25heat/V25HEAT.LBM.json --check-only` — **pass**; 640×480, palette 256, pixels 307,200, raw cycles 16, source hash matches `SOURCE.txt`.
- Explicit temporary validation package — **pass**; `file` recognized a 640×480 8-bit grayscale scene and a 256×1 8-bit RGBA palette, and Python `tomllib` parsed all three generated TOML manifests; that temporary package was deleted.
- Private local package materialization at `fixtures/untracked/effectgames/canvascycle-v25heat/worldwall-scene-package` — **pass**; the same PNG/TOML checks passed, directory/files are mode 0700/0600, and `git check-ignore` confirms the output is ignored. This is the only available CanvasCycle capture and remains local; it was not added to Git.
- `python3 -m py_compile tools/import_canvascycle_capture.py tools/tests/test_canvascycle_import.py` and `git diff --check` — **pass**.
- `python3 tools/check_docs.py` — **pass**, 124 Markdown files, 13 stages, 7 skills.
- `python3 -m unittest discover -s tools/tests -v` — **pass**, 66 tests. The pre-existing CR-0005 live-runner test correctly refused to launch because the persistent Worldwall process is active; no process was started by that test.
- No Rust/runtime code, pool/rules manifest, active registry, config, Firefox process, or Worldwall process was changed or controlled.

## Limitations and next action

S12-AC2 remains open: this is one parser/materializer slice, not the final versioned library/catalog plus pool sidecar, not a complete animated CanvasCycle import, and not a runtime selector/GUI integration. Only one of the 35 selectable CanvasCycle entries has an explicit capture in the inspected local roots and has been materialized. The other entries remain unavailable until local captures are supplied; do not fetch them. Cycle mapping and later preview fidelity remain unverified until suitable local source semantics/evidence are available. Next, define the catalog/library and sidecar model against the approved AC1 source-composition policy, keeping imported assets local and unlicensed until review.
