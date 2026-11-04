# Hyprland session activation — configured time, weather, and animation speed

- Date / operator: 2026-09-23 / Pi coding agent on assigned host `soupydesktop`.
- Authorization: the user requested wiring `hyprland.lua` for automatic login/resume playback and will test reboot. This supersedes the earlier keep-stopped instruction for this deployment; it is not supplemental gate acceptance or permission to reboot/suspend the session.
- Source: `af10d7e090e897c5a818bc39859124d64cc08a58`, plus the launcher/tests/docs in this increment. No Rust source or dependency changes.
- Requirements: [WW-04, WW-06, WW-07, WW-08, WW-10](../delivery-contract.md). Partial evidence for [S04-AC9](../stages/04-palette-animation.md), [S07-AC7](../stages/07-weather.md), [S08-AC4/AC6](../stages/08-suspend-resume.md), [S10-AC9](../stages/10-ipc-config.md), and [S11-AC4](../stages/11-release-validation.md); none is newly accepted here.
- Kind: offline regression tests, live Hyprland/Vulkan observations, private operator deployment, and documentation.
- Environment: Hyprland 0.56.2; Linux `7.2.6-zen2-1-zen`; NVIDIA GeForce RTX 3070 Ti, open driver 615.71.09; Vulkan backend; Rust 1.94.0; locked wgpu 30.0.1, SCTK 0.21.1, wayland-client 0.31.15, calloop 0.14.4, zbus 5.14.0. HDMI-A-1/HDMI-A-2 are 1920×1080 at 60 Hz, scale 1, normal transform; Wayland output IDs 71/72. Existing desktop applications remained active: this is not a controlled resource benchmark.

## Bounded changes and inputs

Tracked additions are [the launcher](../../examples/worldwall-session.sh) and [six offline launcher tests](../../tools/tests/test_worldwall_session.py). README, status/roadmap evidence links, and the [session runbook](../release.md#opt-in-hyprland-lua-session-launcher) now match the deployed state. The scene-format reference and CR-0006 metadata table correct the TOML key to `family`, matching the existing parser; `family_id` remains an internal model name. No policy or acceptance threshold changed.

Private changes, outside Git:

- Installed release binaries under `~/.local/share/worldwall/bin/` and the launcher at `~/.local/bin/worldwall-session`.
- Active `~/.config/hypr/hyprland.lua`: the existing `hl.on("hyprland.start", ...)` handler now calls `hl.exec_cmd("$HOME/.local/bin/worldwall-session")`. Removed only the competing hyprpaper startup and legacy forced-scene launch; other applications remain unchanged. Hyprpaper was stopped only after the persistent Worldwall process was healthy.
- `~/.config/worldwall/worldwall.toml`: schema 4, `fps = 30`, `animation_speed_multiplier = 0.5`, private adaptive registry, existing explicit location/timezone unchanged, Open-Meteo enabled and `adaptive_scenes = true`. Coordinates are not copied into this record.
- `~/.local/share/worldwall/adaptive-registry/`: 18 locally copied variants, 10 families, all five approved temperature bands, clear per-family fallbacks, and V29 offline fallback. Every package has a time-of-day timeline; 16 have authored animation. The two Desert variants have no authored animation, which was not fabricated. The approved Monolith crop is 328×446 with four groups. Source pixel/palette/animation bytes, source fixtures, and guide remain unchanged; the global speed scalar applies once. Deferred Haunted Ruins time-window variants and unavailable V12 are excluded. No new artwork was fetched, bundled, or published.

The launcher makes config authoritative, clears fixed-scene/test overrides, and holds an inherited `flock` across exec. Duplicate launcher calls are successful no-ops. Ordinary per-frame stdout goes to `/dev/null`; runtime errors append to `~/.local/state/worldwall/session-errors.log`. Logind integration stays inside Worldwall: no separate resume hook or service was added. The launcher is not a crash supervisor.

## Artifact identity and retention

The private evidence directory is `$HOME/.local/state/worldwall/hyprland-session-20260923/` (mode 0700), abbreviated **bundle** below. Retain it through reboot/resume review; access is local to the operator. Do not publish its backups/config or private artwork. `registry-artifacts.json` records per-artifact hashes and `materialize_registry.py` records the local-copy procedure. `SHA256SUMS` identifies the retained non-backup evidence files (manifest SHA-256 `e88ae3226550b53ae923df8e5720aa60cde989e3493d3f7715ede50218509fcc`); private backups are deliberately excluded. `final-state.json` confirms the same persistent PID, two backgrounds, 30 FPS / 0.5 config, fresh weather, and empty runtime error log at 00:21 EDT.

| Artifact | SHA-256 |
| --- | --- |
| Installed `worldwall` | `a6ca8b2c9e1c927be762d40fc358599e257c589ecbe0d4f62e95429d24fc4e34` |
| Installed `worldwallctl` | `13964ded094da6e2c8c578a1d70cde3c05156ef2087f04c1bac8b60addd98885` |
| Source and installed session launcher | `11c4125a46b863ac12b48674e2a0c84933394f31f08a6453e7284f1f244ccdd1` |
| Launcher tests | `ceb19b6b7d13e5f31cd08e8a61886143c0d3071b228c7d72cd3761c97546f63b` |

## Commands and observed results

In this section, `CTL=$HOME/.local/share/worldwall/bin/worldwallctl` and `B=$HOME/.local/state/worldwall/hyprland-session-20260923`. Commands were run from the checkout/session, not a clean package installation. Table outcomes describe only the stated check, not whole-gate acceptance.

| Check | Command / procedure | Expected and observed | Outcome / artifact |
| --- | --- | --- | --- |
| Locked release build | `cargo build --offline --locked --release --bins` | Rebuilt both installed binaries successfully. | pass / `build.log`, hashes above |
| Rust regressions | `cargo fmt --all --check`; `cargo clippy --offline --locked --all-targets --all-features -- -D warnings`; `cargo test --offline --locked --all-targets --all-features` | Formatting/Clippy clean; 111 main + 4 CLI tests passed, 3 Vulkan tests ignored, not counted as executed GPU evidence. | pass / `fmt.log`, `clippy.log`, `rust-tests.log` |
| Launcher behavior | `sh -n examples/worldwall-session.sh`; `python3 -m unittest discover -s tools/tests -p test_worldwall_session.py -v` | Six fake-binary/temp-directory tests pass: config/override authority, explicit trace and private permissions, missing config/binary/session/runtime failures, duplicate no-op and lock release after exit. No hardware claim. | pass / test source and full-suite log |
| Lua configuration | `Hyprland --verify-config -c $HOME/.config/hypr/hyprland.lua`; `hyprctl configerrors` | Config accepted, no config errors. | pass / `hyprland-verify.log`, `persistent-checks.json` |
| Live configured startup | `hyprctl eval 'hl.exec_cmd("env WORLDWALL_SESSION_TRACE=$HOME/.local/state/worldwall/hyprland-session-20260923/live-trace.log $HOME/.local/bin/worldwall-session")'` | Full application startup validates the adaptive registry; trace reports configured civil time/astronomy, adaptive V29 fallback, enabled Open-Meteo, Vulkan adapter, and two rendering outputs. One startup provider refresh reaches fresh cache epoch 1 without changing V29 family. | pass / bounded `live-trace.log` |
| Config rate/reload, WW-04/WW-10 | `python3 "$B/sample_live.py" baseline 0.5 30`; edit private FPS/multiplier to 15/0.25, `"$CTL" reload-config`, `python3 "$B/sample_live.py" quarter 0.25 15`; restore 30/0.5, `"$CTL" reload-config`, `python3 "$B/sample_live.py" restored 0.5 30` | Both valid reloads succeeded; V29 scene and status selection generation 1 stayed unchanged. Steady/config-reload upload field stayed 1, uniform counters advanced, phase never moved backward. Rates below track config. | pass / `baseline.json`, `quarter.json`, `restored.json`, `reload-quarter.json`, `reload-half.json`, `phase-monotonicity.json` |
| Explicit full selection, WW-06 | `"$CTL" reload`; `python3 "$B/sample_live.py" adaptive 0.5 30`; `"$CTL" weather` | Explicit reload selected clear Crystal Caves in the Mild band using fresh weather; status selection generation changed to 2. Both outputs kept rendering; weather status remains fresh. This was not an automatic weather-triggered family switch. | pass / `explicit-selection.json`, `adaptive.json`, `weather.json` |
| Power integration prerequisite | Inspect startup `power-service` trace | `state=available resume-support=true`: connected to logind. No physical suspend or missing-service experiment in this increment. | pass for connection only / `live-trace.log` |
| Persistent exact startup command | End diagnostic PID 5312 with SIGTERM, then `hyprctl eval 'hl.exec_cmd("$HOME/.local/bin/worldwall-session")'`; `"$CTL" status`; `hyprctl -j layers` | PID 5885 runs installed binary, persisted Crystal Caves, fresh weather, 30 FPS, no degraded outputs, one Worldwall background on each monitor, no hyprpaper. SIGTERM is not evidence of graceful teardown. | pass / `persistent-status.json`, `persistent-checks.json` |
| Duplicate/reload safety | Three direct launcher invocations and one repeat of the same `hl.exec_cmd`; `hyprctl reload config-only`; inspect process, layers, `/proc/5885/fd`, and environment | All four duplicate attempts leave PID 5885 unchanged; config-only reload adds no process/error. FD 9 holds lock, FD 1 is `/dev/null`; explicit config present, forced scene absent. Runtime error log empty at final check. | pass / `persistent-checks.json` |
| Documentation/tool regression | `python3 tools/check_docs.py`; `python3 -m unittest discover -s tools/tests -v`; `git diff --check` | 119 Markdown files / 12 stages / 7 skills pass, archive intact; all 55 Python tests pass (including six launcher tests); diff check clean. | pass / `docs-check.log`, `python-tests.log`, `launcher-tests.log` |
| Reboot/next login | User's planned reboot | Not performed by agent. Exact command works in the existing session; a real login event is a separate test. | not-run |
| Physical resume and same-family weather swap | Authorized physical sleep/wake and fresh changed-precipitation observation | Neither performed here. Connection and unit tests cannot substitute for these gates. | not-run |

### Bounded rate observations

Each phase samples approximately six seconds; receive timestamps have approximately 5 ms polling precision. These are sanity checks, not S11's warmup/three-window resource benchmark or a full visual/color review.

| Config / scene | Output 71 frames / observed FPS / phase seconds per real second | Output 72 frames / observed FPS / phase seconds per real second |
| --- | --- | --- |
| 30 FPS, 0.5, V29 | 177 / 29.628 / 0.500295 | 178 / 29.796 / 0.500296 |
| 15 FPS, 0.25, V29 | 90 / 14.982 / 0.249892 | 90 / 14.982 / 0.249892 |
| Restored 30 FPS, 0.5, V29 | 180 / 29.943 / 0.499955 | 180 / 29.943 / 0.499954 |
| 30 FPS, 0.5, Crystal Caves | 179 / 29.744 / 0.499585 | 180 / 29.911 / 0.499584 |

Sampler assertions check both outputs, stable scene/generation within each window, expected configured FPS/rate, advancing uniform counters, stable upload field, and minimum phase spacing with rounding tolerance. A separate scan of all 4,764 diagnostic frame records found no backward phase step, including rate changes. An upload field of 1 after explicit selection is only a steady-window observation: a new renderer/resource epoch can reset counters, so this is not a claim of zero uploads during scene replacement.

## Limitations and failures

- CR-0006/CR-0007 supplemental acceptance remains open. This does not complete S04-AC9's visual/suspend matrix, S07-AC7's same-family swap, S08-AC6's resume/hysteresis matrix, or the full S05/S10 async failure/latency evidence. The earlier accepted S08 physical cycles remain separate historical evidence.
- The configured clock and timeline inputs loaded; no accelerated noon/night visual test or multi-scene color review was performed. Desert remains source-static except for its time palette. No performance/power saving or universal hardware claim is made.
- Earlier CR-0005 synthetic-atlas visual failure, aborted source-preserving run, and output-115 pacing question remain unchanged. Current IDs 71/72 and these bounded samples do not retroactively finalize that run.
- An initial `hl.dsp.exec_cmd(... )()` eval attempt returned a dispatcher-not-callable error and launched nothing; the supported `hl.exec_cmd(...)` path succeeded. The first full Python run occurred before this update file existed: two docs-link checks failed on the missing target, while all six launcher tests passed. The interim log is retained as `python-tests-interim-missing-update.log`; the subsequent final checker and all 55 tests passed with this file present.
- Normal stdout suppression trades full per-frame diagnostics for bounded unattended disk use. Error logging is append-only, not automatic rotation. The launcher does not supervise crashes or coordinate a separately launched raw binary.

## Rollback and next owner

Private pre-change backups are at `bundle/backups/{hyprland.lua,worldwall.toml,worldwall}`. The config backup contains private location data; keep it local. If rollback is requested, stop only the confirmed Worldwall PID, restore the config/Lua backup and (if necessary) the binary while stopped, restore its executable bit, then validate/reload Hyprland. A config reload does not rerun startup handlers: launch the desired restored wallpaper explicitly or log in again. Do not delete the held lock or the private adaptive registry to roll back. No rollback was performed; Worldwall is intentionally left running.

- Reviewer / date: not reviewed for supplemental acceptance.
- Decision: deployment and bounded checks completed under the user's session-activation request; historical stage states unchanged.
- Next owner / action: user tests reboot/next login. A separately authorized physical suspend/resume check and same-family precipitation-swap evidence remain follow-ups; the assigned maintainer reviews the supplemental gates.
- Post-login status: `~/.local/share/worldwall/bin/worldwallctl status`; runtime errors: `~/.local/state/worldwall/session-errors.log`.
