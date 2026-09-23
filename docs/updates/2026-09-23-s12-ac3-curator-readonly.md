# S12-AC3 increment — read-only local curator slice

- **Date:** 2026-09-23
- **Stage / requirements / gate:** S12; WW-03, WW-04, WW-06, WW-07, WW-08, WW-10, WW-12; S12-AC3 (partial implementation only)
- **Owner:** Pi coding agent
- **Acceptance authority:** `simplysoup`, per approved contract v0.8
- **Baseline revision:** `931c89f` (`S12: add catalog/sidecar model and record AC2 acceptance`)
- **Source state:** uncommitted working-tree increment. Core source hashes: `src/curator.rs` `d7f087cd58839becff068ca6a10e55fc356c3b9bb7614037263fd3ca80f99d11`; `src/bin/curator.html` `11020c92fed3b4dac522998c363767ec51b240717d4b24ed02b5cbde408e3735`; `src/bin/curator.js` `aebdf200901fe03942e1711d3bb7fa10034c75b7c96254defcc9eb94ad436838`; `src/bin/worldwallctl.rs` `9e3608d89fcc86b0dc1d4dd41c14f1b12df582e18a5f4e19ca4f0cab95a7f6b7`. Built local `target/debug/worldwallctl` SHA-256: `eadef707c79a0ef86c29ea9c85fed52bfd34929b57df3613b10d3eb2f7196edb`.

## Implemented slice

- Added `worldwallctl curator [--library DIR] [--rules FILE]` with conventional XDG defaults selected for this implementation slice: `$XDG_DATA_HOME/worldwall/scene-library` (fallback `$HOME/.local/share/worldwall/scene-library`) and `$XDG_CONFIG_HOME/worldwall/scene-pools.toml` (fallback `$HOME/.config/worldwall/scene-pools.toml`). Relative XDG base values fall back; explicit options override. A missing default rules file is treated as absent, while an explicitly named missing rules file is an error.
- Added a real external-Firefox page with searchable/scrollable catalog entries, actual catalog metadata, selected-scene detail, and a read-only sidecar summary. The UI uses DOM text nodes for catalog values and does not use the concept mockup's synthetic art.
- The selected preview decodes the indexed PNG and 256×1 palette through the existing S03 scene decoder, transfers raw index bytes and palette data to a WebGL2 fragment shader, and resolves palette colors on the GPU with nearest sampling. No CPU pixel recoloring or renderer/runtime integration was added.
- Catalog/package loading bounds TOML/PNG sizes, rejects malformed IDs, duplicate catalog IDs, unknown schema keys, traversal-like filenames, and symlinked package/asset files, and checks `scene.toml` identity/references. The sidecar summary checks schema, pool membership uniqueness, and basic rule structure. The complete AC2 Python validator remains the full digest, provenance, package-set, and rule validator; the curator does not recompute asset SHA-256 values.
- Firefox is launched only after catalog/rules preflight, with `--no-remote` and a temporary mode-0700 profile. The profile is removed after Firefox exits and contains mode-0600 `user.js` preferences disabling telemetry/update/remote-configuration features for that profile. UI requests are token-scoped GETs over an ephemeral `127.0.0.1` listener, serving only catalog data and decoded assets. No Worldwall IPC, runtime/session/config changes, weather request, artwork publication, or Worldwall process control was performed.

## Changed files

- Implementation: `src/curator.rs`, `src/bin/curator.html`, `src/bin/curator.js`, `src/bin/worldwallctl.rs`. No Rust dependency or lockfile changed.
- User/format documentation: `README.md`, `docs/ipc.md`, `docs/scene-pools.md`, `docs/scene-format.md`.
- Progress records: `docs/status.md`, `docs/roadmap.md`, `docs/changes/README.md`, `docs/delivery-contract.md`, `docs/stages/12-scene-pool-curator.md`, `docs/updates/README.md`, and this update.

## Validation

- `cargo fmt --all --check` — **pass**.
- `cargo clippy --offline --all-targets --all-features -- -D warnings` — **pass**.
- `cargo test --offline --all-targets --all-features` — **pass**: 111 application tests passed, 3 Vulkan readback tests ignored as annotated; 47 `worldwallctl` tests passed (including catalog/path/PNG/HTTP/sidecar/profile cases).
- `cargo build --offline --locked --all-targets --all-features` — **pass**.
- `python3 tools/check_docs.py` — **pass**, 127 Markdown files, 13 stages, 7 skills.
- `python3 -m unittest discover -s tools/tests -v` — **pass**, 96 tests.
- `node --check src/bin/curator.js` and `git diff --check` — **pass**.
- `target/debug/worldwallctl curator --help` — **pass**, printed command usage without launching a browser.
- `target/debug/worldwallctl curator --library fixtures/untracked/effectgames/canvascycle-v25heat --rules /tmp/worldwall-nonexistent-sidecar.toml` — exited 1 with the expected missing-explicit-sidecar error after catalog preflight. It did not start Firefox or write the sidecar.

## Limitations / gate disposition

This is **not S12-AC3 acceptance**. No Firefox curator window was launched in this increment, so the application UI's real Firefox rendering, keyboard/scroll behavior, and assigned-Hyprland lifecycle remain not-run; AC1's earlier host spike does not substitute for this application test. The preview is intentionally static: this locally available V25HEAT capture has source-native cycles whose animation semantics remain unmapped, and no cycles are fabricated. Pool/scene controls, criteria editing, preview-snapshot eligibility explanations, authored animation, ToD scrubbing, and save behavior are still absent. Asset digest verification is delegated to the AC2 validator, and preview output has not been compared against the accepted GPU reference (AC6). AC4–AC7 remain open; S12 remains `in-progress`.

**Next safe action:** extend the same UI slice with in-memory pool/scene controls and explicit preview-snapshot eligibility reasons, without writing config or causing runtime selection; then perform a bounded Firefox/Hyprland application launch/input/close test when safe. Do not animate V25HEAT's unmapped cycle records or claim AC3 complete before the missing controls, animation, eligibility explanations, and host evidence exist.
