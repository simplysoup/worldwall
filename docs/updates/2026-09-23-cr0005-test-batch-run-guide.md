# CR-0005 test batch and live-run guide

**Date:** 2026-09-23
**Requirements:** CR-0005; S04-AC6..8, S06-AC5, S07-AC6, S10-AC6
**State:** offline batch passed; the synthetic-atlas visual was reported `fail`; a later source-preserving V29 display looked good to the user but its runner was aborted before full technical finalization. Output 115 pacing from the earlier run needs review.

## What the batch covers

The offline batch combines deterministic CPU/config/service tests with one explicitly invoked Vulkan readback. It also builds the debug binaries and runs project documentation/tool checks. It does not launch Worldwall during the offline phase, stop a running instance, change the compositor/session setup, or contact Open-Meteo.

Here, “solar-v2” means the optional CR-0005 timeline whose keyframes are anchored to calculated sunrise/sunset events. That is distinct from Effect Games V29's existing schema-v1 civil-time timeline and graded palette atlas. The private V29 scene and future live fixture keep the Effect Games data unchanged; solar-v2 remains separately covered by offline tests.

Automated cases cover:

- Solar-v2 event-boundary vectors, signed offsets, daily sunrise movement, midnight wrap, duplicate/unavailable/polar/unbracketed/overflow fallback, and malformed solar-manifest variants while retaining schema-v1 civil vectors.
- Toronto IANA spring/fall DST conversion.
- Transactional location/timezone/provider replacement using a fake weather provider, cancellation of the old in-flight worker before the new fetch observes it, the new configured location, scoped generation replacement, and resident-scene/selection-generation preservation. The fake provider does not perform HTTP.
- Vulkan readback of a CPU-resolved solar row pair/fraction, base fallback, and an ordinary brightness-pulse effect over the sampled solar palette.
- Private live-fixture generation and privacy properties: the copied package retains the source schema-v1 `time_of_day.toml` and 21-row `time_of_day.png` byte-for-byte, the test config explicitly disables weather, source files remain unchanged, and private files/directories are restrictive.
- Live-runner process guards, explicit confirmation, private log-file modes, and per-output contiguous-FPS-phase pacing-metric grouping.

## Offline run

Run from the repository root:

```sh
python3 tools/run_cr0005_tests.py --prepare-live
```

The script creates a new mode-`0700` directory under `~/.local/state/worldwall/` (or a path supplied with `--output-dir`). Each command writes its full combined output to a separate mode-`0600` log; `manifest.json` records command, exit status, and log name. Cargo commands use `--offline`, and inherited `WORLDWALL_*` variables are cleared. The explicit offscreen test is separate from normal `cargo test`; it has its own `gpu-solar-readback.log`.

The bundle also contains `effectgames-time-of-day-scene/` and `test-config.toml`, both private. Fixture preparation reads only explicit location/timezone fields from the selected private config and writes them only into that private test config—not command logs, manifest, or tracked project records. The generated live config sets weather to `disabled`. Preparation itself does not launch the application.

Check every row in `manifest.json` and its corresponding log. A failed command does not prevent later checks from running. Exit `0` means the automated batch commands passed; it does not by itself mean live visual/pacing evidence is complete or a stage gate has been accepted.

## User-intervention boundary for assigned-session testing

The live check runs a private copy of the source Effect Games V29 package with its original civil-time palette atlas on the current Wayland session, records frame/uniform/upload traces, exercises 30→15→30 FPS config reloads, rejects an invalid config, checks weather-disabled status, and exits by a debug-only deadline. It does not send an Open-Meteo request.

1. Choose a session window in which a temporary background change is acceptable.
2. If any Worldwall process is active, **stop it yourself only when safe** using your normal session workflow. The runner never stops, signals, or replaces a pre-existing Worldwall process; it records `blocked-active-worldwall` and exits if one is found.
3. Use the private bundle path and explicitly authorize the bounded launch:

   ```sh
   python3 tools/run_cr0005_live.py \
     --bundle "${XDG_STATE_HOME:-$HOME/.local/state}/worldwall/cr0005-test-batch-REPLACE_WITH_RUN_ID" \
     --confirm-live-run
   ```

   The runner rechecks for active processes before launch, requires the current Wayland socket, builds the current debug binaries offline, and launches only its own child. The system wall clock drives the copied civil-time timeline by default; `--utc-seconds` explicitly pins the debug-only test clock. The child has an automatic deadline; cleanup is limited to that child.
4. Observe the image and pacing during the run. The runner does not take a screen capture. In an interactive terminal, answer `pass`, `fail`, or blank (`not-run`) at the end; it records only the choice and timestamp in `visual-observation.json`.
5. Review `live-result.json`, `worldwall-live.log`, `frame-observations.csv`, `frame-pacing-by-output-fps.json`, every `status-*.json`/`reload-config-*.log`, and `live-build.log`. The native civil-time timeline and atlas hashes are checked against the prepared bundle manifest. Frame timestamps are harness-observed monotonic log times, not direct hardware scanout timestamps. Status currently labels an environment-loaded package as `synthetic-reference`; use `trace scene-source=package` and the renderer upload/frame traces—not that status field—for scene identity.

The default 90-second duration allows the live config checks at approximately 15, 28, and 42 seconds. Duration overrides are limited to 60–300 seconds. The runner restores the private test config to 30 FPS after the child exits, including handled interrupts, and records restoration errors. Weather remains disabled in that config.

## Run record — 2026-09-23

Two distinct bundles are retained:

- The earlier synthetic solar-v2 run bundle `cr0005-test-batch-20260923T011740Z` is retained in private state outside the repository. Its eight-command offline batch passed: 95 main Rust tests and 4 CLI tests passed, 3 GPU-only tests were ignored in the general run, the explicit solar Vulkan readback passed, and format/Clippy/offline build passed. That batch ran 48 Python tooling tests and checked 109 Markdown files. After the pacing-summary enhancement, 49 Python tests and `py_compile` passed; final evidence docs checked 110 Markdown files and `git diff --check` passed. Logs are retained in this bundle.
- After the user reported the purple appearance, the source-preserving offline batch `cr0005-test-batch-20260923T014545Z` in private state outside the repository passed all eight commands: 95 main Rust tests, 4 CLI tests, 3 GPU-only tests ignored in the general run, explicit solar Vulkan readback, format, Clippy, offline build, 49 Python tooling tests, docs check over 111 Markdown files, and `git diff --check`. It prepared `effectgames-time-of-day-scene/`, retained both source time-of-day files byte-for-byte, and did not launch Worldwall. The manifest records their hashes.
- A first live-run attempt failed before `Popen` because the runner called `.chmod()` on a `TextIOWrapper`. No Worldwall child was launched by that failed attempt. The fix applies mode `0600` to the log `Path` before opening it; `test_live_runner_opens_private_log_files` covers the regression. See [FM-0014](../knowledge/failures/FM-0014-python-log-stream-permissions.md).
- The earlier 90-second assigned-session run used the synthetic solar-v2 atlas and exited successfully at the process/control level. The log reports `scene-source=package`, two ready outputs, a solar sample of rows 3→0, `weather-enabled=false`, and `shared-scene-uploads=1` throughout the recorded frames. Its all-purple visual result was reported by the user and is now a `fail`; the 2026-09-23T014545Z corrected source-preserving fixture has not been run live. Valid config reloads changed FPS 30→15→30 with `reselected=false`; invalid FPS 60 was rejected, status remained at 15 FPS after rejection, and selection generation remained unchanged.
- The run recorded 2,299 frame lines and animation-uniform updates from 2 to 2,267. Per-output, per-contiguous-rate-phase results are in `frame-pacing-by-output-fps.json`: output 114's first 30 FPS phase had 456 frames (median 33.38 ms, p95 33.82 ms), its 15 FPS phase had 405 (66.72 ms, 67.07 ms), and its later 30 FPS phase had 1,405 (33.38 ms, 33.76 ms). Output 115 had 27 frames during its initial 30 FPS phase (median 100.27 ms, p95 3,070.55 ms) and 6 during the 15 FPS phase (468.22 ms, 1,134.90 ms); it had no recorded frames in the later 30 FPS phase. The tool sets no acceptance threshold; the sparse output-115 trace needs review (including whether that output was visible/active) and must not be presented as a pacing pass. These are log-emission intervals, not physical scanout measurements.
- The first assigned-session run used an artificial four-row solar-v2 atlas that replaced the Effect Games V29 palette only in the private test bundle. The app trace sampled rows 3→0; those test rows are blue-heavy, and the user reported the scene looked purple/all wrong. `visual-observation.json` and `live-result.json` record this as a user-reported `fail`; no screenshot was captured. Given the test configuration and logged row pair, this visual failure is consistent with the synthetic test palette; it does not by itself show that the byte-identical private V29 civil-time palette is purple. See [FM-0015](../knowledge/failures/FM-0015-synthetic-atlas-overrides-source-palette.md). The old `solar-v2-scene/` bundle remains unchanged for reproducibility; do not rerun it as a visual demo. The later source-preserving run used `effectgames-time-of-day-scene/`; the app log confirms `scene-source=package`. Its runner was aborted before writing final metrics/clean-exit results; `live-result.json` records the incomplete run, while `visual-observation.json` records the user's `pass` report. The private test config was restored to 30 FPS/weather-disabled; no operator config was changed. The earlier synthetic run's IPC `scene` field says `synthetic-reference` and is not used for identity.
- Weather was disabled in the test config. No Open-Meteo request was made. The private config was restored to 30 FPS.

The synthetic run's `live-result.json` control/upload checks passed, but that is not a visual acceptance claim; its visual result is `fail` and output 115's pacing remains unresolved. The user explicitly accepted the CR-0005 add-on separately; that acceptance does not change the evidence outcomes above.

## Evidence limits and next action

- CPU/fake-provider tests and Vulkan readback are not assigned-session visual evidence.
- The assigned-session run produced real Wayland/Vulkan traces, but its user-reported visual result is `fail` for the synthetic palette. Output 115's sparse pacing observations still require review.
- A screenshot alone cannot replace callback/pacing/upload traces or the user's visual check.
- Test-runner results do not change stage acceptance or the CR-0005 decision. Keep observed evidence separate from maintainer acceptance; do not claim an unrun visual check passed.

Next action: clarify whether the user wants the staged Hyprland Lua callback made active for startup. The current config has `registry_dir = "none"` and points at one V29 package, so weather may refresh cache/status but cannot select another scene. The earlier output-115 pacing trace remains under review.
