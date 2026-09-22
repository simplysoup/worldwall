# 2026-09-22 — S11 release package and operator runbook increment

## Scope and ownership

- Stage / requirements: S11; S11-AC1, S11-AC4, S11-AC5 preparation; WW-12.
- Outcome: establish a reproducible package layout and an operator runbook
  without publishing, installing, or changing session configuration.
- Owner: Pi coding agent. Acceptance authority: `simplysoup`.
- Baseline: S10 accepted; renderer, selection, IPC, and pacing boundaries were
  not changed.

## Implemented slice

- Added `tools/package_release.py`, which performs a locked release build,
  copies `worldwall` and `worldwallctl` plus the required operator/contract
  documents, rejects fixture/artwork paths, and writes a deterministic
  per-file/artifact SHA-256 manifest.
- Added `examples/worldwall.toml` as a secret-free, no-registry configuration
  example. No scene package or artwork is bundled.
- Added `docs/release.md` with foreground launch, controls, config/privacy,
  recovery/troubleshooting, session-environment, and package limitations.
- Added package helper tests and linked the runbook from the documentation and
  tooling maps.
- Moved S11 from `planned` to `in-progress` in the roadmap with the Pi coding
  agent as owner. This is not a stage acceptance decision.

## Verification

| Criterion / requirement | Exact command or artifact | Result | Outcome |
| --- | --- | --- | --- |
| S11-AC1 preparation / WW-12 | `cargo fmt --check`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo test --locked --all-targets --all-features` | Format and clippy passed. Rust tests passed: 82 passed, 3 ignored GPU adapter-gated tests; the binary target tests passed: 4 passed. | pass for the existing local build/test baseline; clean-checkout acceptance remains open |
| S11-AC1 preparation / S11-AC5 | `python3 tools/package_release.py --output-dir target/worldwall-release --allow-dirty --force` | Release build and package smoke passed. Manifest artifact digest: `45ed1507936e287a520c6e452d12a00fd70d43c36488ffeb5fadf9424976906d`. It contains both binaries, eight documentation/config/license inputs, and `bundled_artwork=[]`. | pass for a local dirty-tree smoke package; not a clean-checkout release result |
| S11-AC4 preparation | `python3 tools/check_docs.py`; `python3 -m unittest discover -s tools/tests -v` | Documentation checker passed: 93 Markdown files, 12 stages, 7 skills; archive intact. Python tests passed: 41 tests. | pass for tracked documentation/tool behavior |
| WW-12 package boundary | `tools/tests/test_package_release.py` and package inspection | Fixture/artwork paths are rejected; entries are sorted and hashed; package file list contains no scene assets. | pass for the implemented package boundary |

The package was created at
`target/worldwall-release/manifest.json` in the local ignored build tree. The
manifest records `source_revision=96425ac7cf4f2651df5669902dba704ab6f080e7`,
`source_dirty=true`, and `build_profile=release`.

## Limitations and open gates

- The smoke package used `--allow-dirty` because this increment is not yet
  committed. It cannot satisfy S11-AC1's clean-checkout/artifact identity gate.
- No maintainer-approved hardware/workload matrix or numeric CPU/memory/pacing/
  GPU/power/network budgets are recorded yet. S11 performance sampling was not
  run and must not be inferred from the package smoke or prior S04 baseline.
- S11-AC2 fault/recovery execution, assigned-session release validation,
  full-upload/selection/network invariant sampling, licensing review, and
  maintainer release decision remain open.
- The package tool does not publish or install artifacts. A user service is not
  supplied; foreground launch and a tested Hyprland `exec-once` environment
  remain the documented path.

## Next safe action

Obtain and record the maintainer-approved named hardware/workload matrix and
numeric release budgets before running the S11 measurement procedure. In
parallel, execute the required assigned-session recovery matrix and retain raw
logs under the S11 evidence directory; missing hardware results remain
`not-run` or `blocked`, never passes.

## Review and acceptance

- Reviewer / date: not reviewed
- Decision: implementation increment only; S11 remains in progress
- Unmet criteria: S11-AC1 clean-checkout release identity, S11-AC2,
  S11-AC3, S11-AC5 final license/release decision
- Next owner / action: Pi coding agent, pending maintainer matrix/budget input
