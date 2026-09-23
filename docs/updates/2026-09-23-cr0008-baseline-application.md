# CR-0008 — Approved baseline application

- Date: 2026-09-23
- Approval: `simplysoup` explicitly approved CR-0008 in conversation on 2026-09-23, with the clarification that pool and scene-availability rules affect every automatic selection path; manual `scene <id>` is the sole override.
- Baseline: delivery contract v0.8. This application adds post-M4 S12; it does not change M4 acceptance or historical S00–S11 states.
- State: baseline records applied; S12 is `ready`, owner unassigned, and implementation/acceptance have not started.

## Applied scope

- Added the approved S12 gates and roadmap/status entries for the private curator library, GUI command, optional schema-v5 pool sidecar, and runtime filtering.
- Recorded that all automatic scene selection (startup/full selection, resume, explicit `reload`, `next`, and existing CR-0006 same-family weather variant changes) must choose enabled, eligible scenes. Manual `scene <id>` bypasses pool/availability filters while retaining asset validation.
- Preserved the existing trigger matrix: no new selection on clock/weather refresh or `reload-config`; applying a pool sidecar does not itself select. Missing sidecar preserves legacy behavior. No hidden ineligible fallback is allowed when the sidecar is active.
- Kept S12 outside M4 and CanvasCycle packages separate from the adaptive registry. No runtime code/config change, asset import, artwork distribution, provider request, or hardware test was performed for this record.

## Files

- `docs/delivery-contract.md` — v0.8 and WW/S12 scope.
- `docs/stages/12-scene-pool-curator.md` — stage gates.
- `docs/roadmap.md` and `docs/status.md` — S12 ledger and unstarted status.
- `docs/architecture.md`, `docs/ipc.md`, and `docs/scene-format.md` — approved target boundaries, clearly marked unimplemented.
- `docs/changes/CR-0008-worldwallctl-scene-curator.md` and `docs/changes/README.md` — approval/application record.
- `docs/mockups/canvascycle-scene-curator.html` — synthetic concept only.
- `tools/check_docs.py` — registers S12 with the documentation checker.

## Validation

- `python3 tools/check_docs.py` — passed; 122 Markdown files, 13 stages, 7 skills.
- `python3 -m unittest discover -s tools/tests -v` — 55 tests passed. The CR-0005 live-runner test did not launch because a Worldwall process was already active; no process was launched by this application.
- `git diff --check` — passed.
- The local HTML concept opened in the browser; pool controls and explanatory preview text were inspected. This is design evidence only. No GUI application, runtime selector, Wayland/GPU, or hardware acceptance was tested.

## Revision

- Baseline application commit: to be recorded after commit creation.
