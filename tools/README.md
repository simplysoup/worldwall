# Documentation checks

These are developer tools, not the Worldwall application. They require Python
3.10+ (3.11+ for `scene_pool_library.py`, which uses the standard-library
`tomllib` parser) and only the standard library. Run from the project root:

```sh
python3 tools/check_docs.py
python3 -m unittest discover -s tools/tests -v
```

The checker exits `0` on success and `1` on findings. `--root PATH` checks a different project copy; the default root is relative to the script, not the caller's working directory. It makes no network requests or file changes.

The S02 nested lifecycle harness is `run_nested_s01.sh`. On a Wayland host with Hyprland, it compiles the test-only `nested_wayland_connect.c` bridge with the available C compiler so the child compositor can publish a distinct socket while using the outer compositor as its backend. The harness rejects outer-socket reuse and waits for a GPU presentation before output-removal or disconnect teardown.

## CanvasCycle local capture importer

`import_canvascycle_capture.py` accepts only an explicitly supplied local legacy
LBM object-literal capture. It parses a bounded data-only grammar; it never
executes JavaScript or fetches a source URL. `--check-only` reports the source
hash and structural counts without writing files. Materialization requires an
explicit new output path, stable ID, and provenance URLs/title. Output
`scene.png`/`palette.png` assets and `scene.toml` are placed in a mode-0700
package with mode-0600 files; existing output is never overwritten. Provenance
records `license_state = "unreviewed"`. Source cycles are mapped to signed
authored-byte `blend_shift` `animation.toml` groups using the Effect Games
`Palette` semantics established in
[FM-0013](../docs/knowledge/failures/FM-0013-effectgames-cycle-rate.md):
`speed = rate * source_speed_adjust / 280 / member_count`, where
`--source-speed-adjust` (default `1.0`, the site's base speed control) is the
site's speed multiplier and source reverse mode `2` signs the speed as
reversed. Unrepresented source modes are rejected, and no time/weather rules
or source-title-derived eligibility is created.

Example local-only validation:

```sh
python3 tools/import_canvascycle_capture.py \
  --input fixtures/untracked/effectgames/canvascycle-v25heat/V25HEAT.LBM.json \
  --check-only
```

The private capture under `fixtures/untracked/` is ignored and is not part of a
clean checkout. Other CanvasCycle entries must not be synthesized or fetched;
import only entries for which an operator explicitly supplies a local capture.
The generated art is private inspection material and must not be committed,
published, or redistributed without a separate license review.

## Scene-pool library catalog and sidecar validator

`scene_pool_library.py` validates the S12 library catalog (`library.toml`) and
pool/scene-rules sidecar (`scene-pools.toml`) defined in
[docs/scene-pools.md](../docs/scene-pools.md). It needs Python 3.11+ for the
standard-library `tomllib` parser. It reads only explicitly supplied local
files and writes nothing except an atomic `--write-catalog` replacement of
`<library>/library.toml`; a validation failure preserves any previous catalog.
Paths, file counts, file sizes, image dimensions, palettes, and source-cycle
collections are bounded, and duplicate IDs, unsafe paths, symlinks, malformed
rules, and invalid animation/cycle data are rejected before any write.

```sh
# Validate an existing catalog (and optionally a sidecar); writes nothing.
python3 tools/scene_pool_library.py \
  --library fixtures/untracked/effectgames/canvascycle-v25heat \
  --sidecar /path/to/scene-pools.toml \
  --check-only

# Rebuild library.toml from the complete packages in the library root.
python3 tools/scene_pool_library.py \
  --library fixtures/untracked/effectgames/canvascycle-v25heat \
  --write-catalog
```

Every non-hidden subdirectory of the library root must be a complete package
(`scene.toml`, `provenance.toml`, and exactly the declared assets); unrelated
regular files are ignored. Curated `world`/`appearance` relationships survive
regeneration and are never inferred from source titles. License states and
unmapped source-cycle semantics are recorded as declared and never upgraded.
The runtime consumes the same two formats through `src/scene_pools.rs`
(schema-v5 `scene_pools_file`); this tool remains the full
digest/provenance/package-set validator.

## Preview fidelity goldens (S12-AC6)

`fixtures/preview-palette-goldens.json` locks the accepted CPU reference
(`Animation::apply_palette`) and the exact GPU metadata/cycle byte payloads for
representative static/hue/pulse/cycle/BlendShift inputs. The Rust test
`preview_palette_goldens_match_cpu_reference_and_byte_payloads` verifies and
can regenerate them:

```sh
WORLDWALL_WRITE_PREVIEW_GOLDENS=1 cargo test --offline preview_palette_goldens
```

`tools/tests/test_preview_palette_fidelity.py` verifies the curator's
JavaScript palette twin (`src/bin/curator_palette.js`) against the same goldens
with Node.js; the WebGL2 preview compares its shader readback against that twin
at runtime. `tools/tests/test_curator_pool_defaults.py` exercises the pure
`src/bin/curator_pools.js` helper so a missing sidecar groups library and
registry scenes into source-correct pools before the first save.

## Live pool-mode smoke run (S12-AC4/AC5)

`run_s12_pool_smoke.sh` is the re-runnable, maintainer-authorized live smoke of
schema-v5 pool consumption and the shared selection filter. It assembles a
temporary smoke registry/library/sidecar from the private untracked derived
fixtures (clearly labelled local test material), starts a nested Hyprland on
the outer session through `nested_wayland_connect.c`, and runs the real
`worldwall` with a private `XDG_RUNTIME_DIR` (distinct control socket, absolute
`WAYLAND_DISPLAY`) and `[weather]` disabled. It asserts pool-filtered startup
and `next` cycling, the manual-ID override, no-selection `reload-config`, the
`next` no-op reason, invalid-sidecar rollback, fail-closed no-eligible startup,
and known-input match/mismatch criteria, and it never touches the production
wallpaper, its socket, or session configuration. See the
[live smoke record](../docs/updates/2026-09-23-s12-ac4-ac5-live-pool-smoke.md)
for the recorded phases and limits.

```sh
# From a Wayland session; requires the private fixtures noted above.
WORLDWALL_SMOKE_ARTIFACTS=~/.local/state/worldwall/s12-smoke \
  bash tools/run_s12_pool_smoke.sh
```

`WORLDWALL_SMOKE_ARTIFACTS` optionally retains the phase logs and compositor
captures for evidence records. The preserved `run_nested_s01.sh` assertions target the single-output S01/S02
baseline, so the harness sets the explicit
`WORLDWALL_SINGLE_OUTPUT_BASELINE` diagnostic seam rather than changing the
assertions or production `run_multi()` entry point. The resolution and recorded
passing nested-Hyprland run are documented in
[FM-0016](../docs/knowledge/failures/FM-0016-nested-harness-single-output-traces.md).

## Effect Games matrix runner

`run_effectgames_matrix.py` runs the local, untracked Effect Games registry
sequentially. Each scene gets a fresh Worldwall process, a separate log, and a
JSON manifest; the run also writes `summary.csv` and `summary.json`. The runner
checks the clean debug deadline and owner-safe teardown markers, so a process
that merely exits or times out is not reported as a pass. It does not run scenes
in parallel because they share one Wayland output.

Build first, then run the complete 21-package matrix for a short local check:

```sh
cargo build --locked
python3 tools/run_effectgames_matrix.py \
  --fps 30 \
  --duration-seconds 3 \
  --output-dir "${XDG_STATE_HOME:-$HOME/.local/state}/worldwall/cr003-effectgames-$(date -u +%Y%m%dT%H%M%SZ)"
```

The registry directory defaults to `fixtures/untracked/effectgames`; the
executable defaults to `target/debug/worldwall` or `WORLDWALL_BIN`. Use
`--dry-run` to inspect the resolved matrix without starting the application,
`--scene ID` repeatedly for a subset, and `--fail-fast` to stop after the first
failure. FPS remains limited to the approved 15/30 values.

A JSON config can hold repeatable settings. CLI options override config values:

```json
{
  "registry_dir": "fixtures/untracked/effectgames",
  "binary": "target/debug/worldwall",
  "fps": 30,
  "duration_seconds": 3,
  "timeout_grace_seconds": 10,
  "utc_offset_minutes": 0,
  "output_dir": "/path/to/private-state/worldwall/cr003-effectgames-rerun",
  "env": {}
}
```

Run it with `python3 tools/run_effectgames_matrix.py --config matrix.json`.
The config may also contain a `scenes` string list to replace the registry
matrix. Avoid putting secrets in `env`; explicit environment values are passed
to Worldwall but only their names are recorded in `resolved-config.json`.

## S11 release-candidate package

`package_release.py` creates an identifiable release-candidate directory under
`target/` containing both binaries, operator documentation, the MIT license,
and a minimal config example. It refuses a dirty checkout by default and
records the source revision, dirty state, per-file hashes, and a deterministic
artifact digest in `manifest.json`. It rejects scene fixtures and artwork so
no unlicensed assets enter the package:

```sh
python3 tools/package_release.py --output-dir target/worldwall-release
```

Use `--allow-dirty` only for a local smoke package; use `--force` only to
replace the generated output directory. This tool does not install, publish,
or alter the compositor/session configuration. See the [release runbook](../docs/release.md).

## CR-0005 offline and assigned-session tests

`run_cr0005_tests.py --prepare-live` runs the per-command logged offline batch (format, complete Rust tests, explicit Vulkan solar readback, Clippy, debug-binary build, docs/tool tests, and diff check) and prepares a private weather-disabled copy that preserves the source Effect Games civil-time timeline and atlas byte-for-byte. It never launches Worldwall. See the [CR-0005 test batch run guide](../docs/updates/2026-09-23-cr0005-test-batch-run-guide.md) for the exact evidence files and intervention boundary.

Only after the user has chosen a safe session window and stopped any existing Worldwall instance themselves, run the separate `run_cr0005_live.py --bundle PATH --confirm-live-run`. It refuses to launch if another Worldwall process exists, uses the system wall clock unless a fixed debug time is explicitly requested, uses a debug-only deadline, records application/control/frame logs, checks valid/invalid config reloads, and asks for a separate visual observation. The test config disables weather; neither runner sends an Open-Meteo request or changes compositor/session configuration. Never treat offline tests or screenshots as assigned-session evidence.

## S10 IPC live runner

`run_s10_ipc.py` starts the debug binary on the current Wayland session with a
private runtime directory and exercises the versioned socket against the live
service. It checks stale-socket replacement, `0600` permissions, malformed and
oversized frames, stalled clients, 12 concurrent clients, rapid explicit scene
commands, registry-path transitions in both directions, resident-scene
retention when the registry is disabled, and repeated diagnostic rate limiting.
It records the service log and JSON results under the requested output
directory:

```sh
cargo build --locked
python3 tools/run_s10_ipc.py \
  --output-dir "${XDG_STATE_HOME:-$HOME/.local/state}/worldwall/s10-ipc-live-$(date -u +%Y%m%dT%H%M%SZ)"
```

To exercise the foreign-UID peer check, run the complete runner from the
original desktop user's shell with the session environment preserved:

```sh
sudo -E python3 tools/run_s10_ipc.py \
  --output-dir "${XDG_STATE_HOME:-$HOME/.local/state}/worldwall/s10-ipc-live-root-$(date -u +%Y%m%dT%H%M%SZ)"
```

When started as root, the runner launches Worldwall as `SUDO_UID`/`SUDO_GID`,
keeps the runtime directory owned by that user, temporarily makes only the
test socket reachable, probes it as the foreign root peer, drops the runner
back to the desktop user for all ordinary client checks, and restores mode
`0600` immediately afterward. Do not launch `worldwall` itself as root. If the
host does not permit an unprivileged caller to switch users, the check is
recorded as `not-run`; it must not be described as peer-credential acceptance
evidence. The runner uses only local fixture packages and does not fetch
weather or change compositor/session configuration.

## Checked conventions

- Relative inline Markdown links/images, including percent-encoded paths, angle-bracket destinations, optional double-quoted titles, and heading fragments. Destinations must remain inside the project. Remote URLs are skipped.
- Simple ATX headings (`# Heading`) with lowercase GitHub-style anchors and duplicate suffixes; fenced code is excluded. Links inside inline code are excluded.
- Balanced backtick/tilde code fences with up to three leading spaces.
- Required delivery documents, templates, workflows, seven core skills, and twelve registered stage files. Stages have the standard ordered sections and unique stage-local acceptance IDs.
- Six-column roadmap rows, registered stage links, comma-separated dependencies or `None`, valid states, no dependency cycles, and accepted prerequisites for active/review/accepted stages. Active stages require an owner; accepted stages require an evidence link. Evidence content and reviewer authority require human review.
- Skill `name` and `description` frontmatter, matching directory/name, uniqueness, and discovery-length limits. Use flat, one-line YAML strings: plain, single-quoted, or JSON-compatible double-quoted. Quote values containing YAML punctuation such as `: `.
- The archived original's SHA-256, independently fixed in the checker. Never change its expected hash to make an accidental rewrite pass.

This intentionally is **not a full CommonMark or YAML implementation**. Use inline links rather than reference-style links, ATX rather than setext headings, and fenced rather than indented code. Complex nested Markdown links, raw HTML/custom anchors, YAML block scalars, and remote-link availability are not validated. Extend the parser with regression tests before relying on additional syntax. The checker cannot decide whether prose is consistent, evidence proves a gate, a dependency API exists, or an approval is genuine.

## Maintenance

Keep changes small and add both passing and failing test cases for new rules. An approved stage/skill reorganization must update the registrations in `check_docs.py`, affected docs, and tests together. Do not relax checks to hide a broken link or unaccepted dependency. Generated/dependency directories (`.git`, `.venv`, `venv`, `node_modules`, `target`, `__pycache__`) are excluded from scanning.

See [validation policy](../docs/validation.md) for the separate application and live-hardware checks. These Python tests never advance a software-stage gate by themselves.
