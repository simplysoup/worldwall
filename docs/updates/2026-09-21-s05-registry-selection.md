# S05 registry and selection implementation increment

**Date:** 2026-09-21
**Stage / requirements:** S05; WW-03, WW-06, WW-11
**Owner:** Pi coding agent
**State:** implementation increment; S05 remains in-progress and is not accepted
**Artifact identity:** `src/scene_registry.rs`, SHA-256 `230051c9fb401c4fb6d63338f8fc6294b8b048cfbdfdc908cea617dc17654af4`

## Bounded outcome

Add the dependency-free registry/selection seam without changing the accepted
Wayland, renderer, or S04 pacing path. The increment provides:

- strict `registry.toml` and `scene.toml` parsing with schema and tag checks;
- canonical package containment, bounded manifest/animation reads, asset
  existence checks, and complete S03/S04 asset decoding before a registry is
  considered valid;
- deterministic time/weather/season scoring with injected seed, stable ID
  ordering, explicit manual/next/startup/resume/reload triggers, and ignored
  timer/output/config events; and
- a generation-token CPU transaction boundary that retains the active scene on
  failure or stale completion and reports unchanged content without replacing
  it. The explicit request path now separates request/token creation from
  candidate loading, then commits through the renderer's atomic swap callback;
  stale, unchanged, load-failure, and renderer-swap-failure paths are covered.


The on-disk registry shape used by this increment is:

```text
registry-root/
├── registry.toml                 # schema_version, fallback, scenes = [...]
└── <scene-id>/scene.toml         # package metadata and relative asset paths
```

`SceneRegistry::load` requires at least one listed package and a valid fallback.
`SceneRegistry::empty` exists only as a pure-selector input for the empty
registry result; it is not accepted by the on-disk loader.

## Changed files

- `src/scene_registry.rs` — manifest/package validation, fake environment
  snapshots, selector, explicit trigger controller, content identity, and
  transactional completion tests. `SceneTransaction::complete_with_swap`
  commits the CPU transaction only after the renderer callback succeeds.
- `src/main.rs` — registers the stage-local module.
- `src/wayland_surface.rs` — loads `WORLDWALL_REGISTRY_DIR` at startup through
  a startup transaction and retains the selection state. The explicit
  begin/complete seam returns a cloneable manifest for loading outside Wayland
  dispatch, then routes the completed candidate through
  `Renderer::upload_and_swap_scene`; timer/output/weather/config paths do not
  call it. The fixed `WORLDWALL_SCENE_DIR` and synthetic paths remain
  available.
- `src/renderer.rs` — groups all scene GPU resources into one uploaded bundle
  and adds the renderer-facing `upload_and_swap_scene` boundary. A complete
  candidate is uploaded against the existing bind-group layout before one
  bundle assignment; old resources remain active on upload failure.
- `README.md` — documents the registry startup variable and precedence.
- `docs/roadmap.md` — claims S05 `in-progress` under the Pi coding agent.
- `docs/status.md` — records the bounded active increment and next boundary.
- `docs/scene-format.md` — records the implemented registry-root shape and
  clarifies that the remaining authoring contract is still stage-owned.

## Validation

Baseline before implementation: `cargo fmt --check`, locked clippy, and locked
unit tests passed with 32 tests (30 passed, 2 ignored GPU tests).

After the registry/renderer follow-on:

| Check | Result |
| --- | --- |
| `cargo fmt --check` | pass |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | pass |
| `cargo test --locked --all-targets --all-features` | pass — 39 passed, 2 ignored GPU tests |
| `python3 tools/check_docs.py` | pass — 69 Markdown files, 12 stages, 7 skills |
| `python3 -m unittest discover -s tools/tests -v` | pass — 33 tests |
| Local Wayland/Vulkan startup with a temporary two-package registry and `WORLDWALL_TEST_EXIT_AFTER_MS=1800` | pass — registry startup trace selected a scene, configured the existing renderer, presented 54 frames, kept `scene-upload-count=1`, and exited through the existing owner-safe teardown; observational only, not assigned-host acceptance |

Pure tests cover valid two-package loading, duplicate/unsupported tags,
traversal rejection, missing/bad fallback, repeatable seeded scoring, wildcard
and unknown snapshots, manual ID, next exclusion, ignored non-selection
triggers, stale competing requests, failed candidates retaining the active
scene, unchanged-content no-op replacement, renderer-swap failure retention,
and the guarantee that stale/unchanged completions never invoke the swap
callback. The follow-on also has one local compositor/Vulkan startup trace for
registry loading and the existing renderer path; it is observational and not
assigned-host acceptance.

## Gate status and limitations

- **S05-AC1:** partial evidence in pure tests and the local startup path;
  malformed registry rejection is covered, while assigned-host startup and
  broader asset/error matrix evidence remain outstanding.
- **S05-AC2:** partial evidence in pure tests; deterministic selection,
  manual/next/fallback/empty cases are covered, with later fixture expansion
  still required.
- **S05-AC3:** partial evidence in the explicit controller tests and the
  begin/complete request seam; no public runtime event source exists yet, and
  no timer/output/config path invokes selection.
- **S05-AC4:** partial evidence in CPU transaction tests plus the renderer
  upload/swap implementation and local startup trace. Generation stale work,
  candidate-load failure, unchanged content, and renderer-callback failure
  retain the previous active scene; competing live requests and actual GPU
  upload fault injection remain untested.
- **S05-AC5:** not-run; no M2 demo or reviewer acceptance is claimed.

Artifact hashes for this follow-on are `src/scene_registry.rs`
`230051c9fb401c4fb6d63338f8fc6294b8b048cfbdfdc908cea617dc17654af4`,
`src/wayland_surface.rs`
`e8d44150a5a0e6c7e8771e3709d50761e0916f32105acbfc9eb5eeeaabb7f9e6`, and
`src/renderer.rs`
`229a08411e57b3e7a7bdc245b9cb892bbadb43d34002f215b748c4050564d63d`.
The local trace used temporary copies of the existing untracked provenance
fixtures; no assets were added to the tracked delivery.

No environment refresh, weather, civil-time, IPC, or environment-driven
reselection was introduced. The next safe S05 action is to add the remaining
fixture/error matrix and review the explicit request handoff; a future event
source may call the begin seam, load the cloneable manifest off-thread, and
complete it at a safe renderer boundary without changing ordinary
output/config semantics.
