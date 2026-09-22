# 2026-09-22 — S11 clean-checkout release verification (S11-AC1)

## Scope

- Stage / requirements: S11; S11-AC1 preparation → measured, WW-12.
- Baseline: commit `906aab86584a00d0cae42ed75ab07aaa82a23541` ("Record S11
  release packaging, benchmark dispositions, and result review"), clean tree
  (`git status --porcelain` empty before and after the checks).
- Host: the assigned Hyprland/Vulkan host; this is the only declared matrix
  entry, so this record is a single-host result, not a multi-host claim.

## Verification

| Check | Command | Result |
| --- | --- | --- |
| Format | `cargo fmt --check` | pass |
| Lint | `cargo clippy --locked --all-targets --all-features -- -D warnings` | pass, no warnings |
| Library/binary tests | `cargo test --locked --all-targets --all-features` | 82 passed, 3 ignored (GPU adapter-gated); binary target 4 passed |
| Documentation | `python3 tools/check_docs.py` | PASS: 101 Markdown files, 12 stages, 7 skills; archive intact |
| Tooling tests | `python3 -m unittest discover -s tools/tests` | 41 tests OK |
| Package (no `--allow-dirty`) | `python3 tools/package_release.py --output-dir target/worldwall-release --force` | pass; manifest written |

## Artifact identity

- Manifest: `target/worldwall-release/manifest.json`,
  `source_revision=906aab86584a00d0cae42ed75ab07aaa82a23541`,
  `source_dirty=false`, `build_profile=release`, `bundled_artwork=[]`.
- Packaged inputs (10): `bin/worldwall`, `bin/worldwallctl`, `LICENSE`,
  `README.md`, `docs/delivery-contract.md`, `docs/ipc.md`, `docs/release.md`,
  `docs/scene-format.md`, `docs/validation.md`, `examples/worldwall.toml`.
- `bin/worldwall` SHA-256
  `a75cbb61260a21cb3cf8df791b38a2bef1427f29b571bcd9e363a79670596212`;
  `bin/worldwallctl` SHA-256
  `ea358c3415b40df3d467667cd9e3485db0ceb2df2d31231f3c5292b1d313e8e6` — both
  identical to the binaries sampled in the approved S11 benchmark runs, so the
  benchmark evidence and this package describe the same code.
- Package `artifact_sha256`
  `45ed1507936e287a520c6e452d12a00fd70d43c36488ffeb5fadf9424976906d`. This is
  byte-identical to the earlier `--allow-dirty` smoke package by design: the
  digest covers the sorted packaged file entries only (`path\0bytes\0sha256`),
  not the manifest metadata. Every packaged file's bytes were unchanged between
  the two packagings; the manifests differ in `source_revision` and
  `source_dirty`, which is the difference the smoke/clean distinction records.

## Status and limitations

- S11-AC1: the clean-checkout build/test/package smoke and artifact identity
  are now measured on the declared host. Stage acceptance remains the
  maintainer's M4 decision; this record does not tick the acceptance box.
- S11-AC2 (release-candidate fault/recovery matrix), AC4 (operator-docs
  verification on the packaged layout), and AC5 (licensing/provider-terms
  review and release decision) remain open.
- The three ignored tests are the recorded GPU adapter-gated cases; they pass
  in the assigned-session records cited by earlier stages.

## Review and next action

- Reviewer / date: implementation increment by Pi coding agent; maintainer
  review at the M4 gate.
- Next owner/action: Pi coding agent to execute the S11-AC2 recovery matrix on
  this package (starting from the S10 IPC/config live runner and
  startup/teardown cycles), then AC4 packaged-layout doc verification and AC5
  licensing review.
