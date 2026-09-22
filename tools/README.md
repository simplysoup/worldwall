# Documentation checks

These are developer tools, not the Worldwall application. They require Python 3.10+ and only the standard library. Run from the project root:

```sh
python3 tools/check_docs.py
python3 -m unittest discover -s tools/tests -v
```

The checker exits `0` on success and `1` on findings. `--root PATH` checks a different project copy; the default root is relative to the script, not the caller's working directory. It makes no network requests or file changes.

The S02 nested lifecycle harness is `run_nested_s01.sh`. On a Wayland host with Hyprland, it compiles the test-only `nested_wayland_connect.c` bridge with the available C compiler so the child compositor can publish a distinct socket while using the outer compositor as its backend. The harness rejects outer-socket reuse and waits for a GPU presentation before output-removal or disconnect teardown.

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
  --output-dir /home/soup/.local/state/worldwall/cr003-effectgames-$(date -u +%Y%m%dT%H%M%SZ)
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
  "output_dir": "/home/soup/.local/state/worldwall/cr003-effectgames-rerun",
  "env": {}
}
```

Run it with `python3 tools/run_effectgames_matrix.py --config matrix.json`.
The config may also contain a `scenes` string list to replace the registry
matrix. Avoid putting secrets in `env`; explicit environment values are passed
to Worldwall but only their names are recorded in `resolved-config.json`.

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
  --output-dir /home/soup/.local/state/worldwall/s10-ipc-live-$(date -u +%Y%m%dT%H%M%SZ)
```

To exercise the foreign-UID peer check, run the complete runner from the
original desktop user's shell with the session environment preserved:

```sh
sudo -E python3 tools/run_s10_ipc.py \
  --output-dir /home/soup/.local/state/worldwall/s10-ipc-live-root-$(date -u +%Y%m%dT%H%M%SZ)
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
