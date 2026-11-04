# CanvasCycle V15 live-session runtime repair

- **Date / operator:** 2026-09-24 · Pi coding agent; the user explicitly approved rebuilding, replacing, and restarting the assigned session daemon.
- **Revision / artifact:** source revision `df195af` with a dirty working tree; installed daemon SHA-256 `053a307752c6adc4b5cfc0b872b82cf96c0fb7519f1e81dd6550413aa6816ec0`. The replaced daemon is retained at `~/.local/state/worldwall/worldwall-daemon-pre-canvascycle-v15-20260924-024655` (SHA-256 `f889e611f4b1fe787dbf485ebc72dcb2661c2e3938db5ab4bbd263c6d876bc69`, mode `0600`).
- **Scope:** S12-AC4/AC5 runtime deployment and WW-10 manual scene selection. This is an operational compatibility correction, not new stage acceptance.
- **Environment:** assigned live Hyprland session; Worldwall reports Vulkan, 30 FPS, outputs 71 and 73.
- **Kind:** offline validation plus live compositor/IPC operation.

## Results

| Check | Exact command / procedure | Observed | Outcome |
| --- | --- | --- | --- |
| Baseline config reload | `$HOME/.local/bin/worldwallctl reload-config` against the old daemon | Rejected transactionally with `invalid_config`: its catalog parser treated the curator-added `display_name` key as unknown. Status remained on `v08-jungle-waterfall--clear`, generation 29. | Reproduced |
| Rust validation/build | `cargo fmt --all --check`; `cargo clippy --offline --locked --all-targets --all-features -- -D warnings`; `cargo test --offline --locked --all-targets --all-features`; `cargo build --offline --locked --release --all-targets --all-features` | Format, Clippy, and locked release build passed; 129 application and 77 CLI tests passed; 3 existing Vulkan readback tests were ignored by their adapter gate. | Pass |
| Private catalog validation | `python3 tools/scene_pool_library.py --library $HOME/.local/share/worldwall/scene-library --check-only` | 35 scenes, 175 files, 417 animation groups; license state was not upgraded. | Pass |
| Install / restart | Backed up the old daemon, staged the built release binary in the install directory, atomically renamed it into place, sent SIGTERM to the old session child, then ran `hyprctl eval 'hl.exec_cmd("$HOME/.local/bin/worldwall-session")'`. | New session process started as PID 434676; the current schema-v5 config, pool sidecar, and CanvasCycle catalog loaded. No config, sidecar, or scene package was edited. | Pass |
| Requested scene | `$HOME/.local/bin/worldwallctl scene canvascycle-v15` | `ok: true`, `applied: true`, generation 2. Follow-up `status` reports `scene: canvascycle-v15`, Vulkan, FPS 30, outputs `[71, 73]`, and no degraded outputs. `worldwallctl scenes` marks V15 active. | Pass |

## Limitations and failures

- No screenshot or operator visual review was performed; the evidence proves successful package load and scene transaction/status, not visual appearance.
- No physical resume/hotplug, new acceptance review, license review, or artwork distribution was performed. Artwork/license state remains private and unreviewed.
- Existing S12 acceptance and roadmap state are unchanged. The source checkout was already dirty; no Rust/tool implementation files were modified for this deployment repair.
- Rollback requires stopping the current `worldwall-session`, restoring the retained daemon backup to `~/.local/share/worldwall/bin/worldwall`, and launching the session wrapper again.

## Review and next action

- **Reviewer / acceptance:** not submitted for a new stage decision; no acceptance is claimed.
- **Next action:** leave the repaired session running; retain the daemon backup. Continue with the user's visual check if desired.
