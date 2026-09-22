# S08 suspend/resume orchestration implementation increment

- Date / operator: 2026-09-22 / `soupydesktop` with Pi coding agent
- Stage / requirements: S08; WW-06, WW-08, WW-11; S08-AC1–AC5
- Dependency: S07 accepted. The power-event adapter and paused-clock policy are implemented; operator-authorized multi-cycle sleep evidence and the corrected topology-change-during-sleep subcase have been collected and reviewed.
- Kind: state-machine implementation, system-bus adapter, deterministic tests, live Wayland/Vulkan suspend cycles, and failure-path validation
- Source artifact hashes: `src/power.rs` `f0ec7940672bde626f4327eb514d237ae808bc83968585df91848ef05e44bd09`; `src/scheduler.rs` `b41e7ceb37a00386776bcf0aa02533b2deb9947c59c9a43854b302a13afd71da`; `src/wayland_surface.rs` `ca9cac8382fad8e6db2411310ad7158fc255ab60d741ddb9b5615a5a58906460`; `tools/run_s08_topology.sh` `6b93a1d4c7fc56e7383a124802603b0947c8ea881fff3029ba66432a8ba50390`; `Cargo.toml` `d0c25cad4cb934632ee37c3b11431136c012a055467587341e67c5479b242ab6`; `Cargo.lock` `612e0869e678116091df6ab39b22fa0959418f742cd8fe634db39450b0549527`

## Outcome

Implemented the smallest end-to-end S08 path while preserving the environment → selector → scene resources → renderer → Wayland presentation separation:

- `src/power.rs` contains a deterministic `PowerStateMachine` with explicit active/suspended/resuming phases, service availability/degraded state, coalesced boolean sleep/resume handling, and generation tokens. A second resume is ignored, and a sleep invalidates a pending resume generation.
- The real adapter subscribes to system-bus `systemd-logind` `PrepareForSleep` and `NameOwnerChanged` signals through pinned `zbus` 5.14 APIs. D-Bus work runs on two worker threads and enters the calloop event loop through a bounded channel. Missing/disconnected logind is diagnostic degraded state, not a fabricated resume.
- `FrameScheduler::pause` clears callback/deadline state and `resume` requests exactly one bootstrap frame. The Wayland client drops both animation and weather timers while suspended, ignores late frame callbacks, and reinstalls timers only after a valid output extent is available.
- `PausedAnimationClock` excludes suspended monotonic time and keeps the shader clock bounded to the existing 1024-second phase window.
- Resume refreshes the already available weather/cache snapshot and starts one bounded `RefreshKind::Resume` attempt without waiting on network I/O. It constructs one immutable civil-time/astronomy/weather snapshot, performs one explicit `SelectionTrigger::Resume` request when a registry is active, and commits through the existing transactional renderer swap. Unchanged scene content returns `Unchanged` and does not upload again; failed candidates retain the active scene.
- Explicit location-aware astronomy is available only when `WORLDWALL_LOCATION_LATITUDE`, `WORLDWALL_LOCATION_LONGITUDE`, and `WORLDWALL_UTC_OFFSET_MINUTES` are configured. No host timezone, geolocation, or precise coordinate logging was added.

## Topology subcase procedure increment

The installed Hyprland 0.56.2 session uses its Lua configuration manager, so
`hyprctl keyword monitor ...` cannot mutate monitor rules and returns
`keyword can't work with non-legacy parsers. Use eval.` A reproducible,
operator-gated helper is now tracked at
[`tools/run_s08_topology.sh`](../../tools/run_s08_topology.sh). It captures the
selected non-target output's mode/layout, uses `hyprctl eval hl.monitor(...)` to
disable that output after the real `PrepareForSleep(true)` signal, verifies the
compositor reports `disabled=True`, restores the captured rule after
`PrepareForSleep(false)`, and verifies `disabled=False`. It deliberately
changes a non-selected output because S08's single-output lifecycle still
terminates when its selected output is removed; selected-output survival and
zero-output recovery belong to S09.

The reversible command syntax was checked on the assigned Hyprland session:
Lua `eval` returned `ok`, the output transitioned to disabled, and the exact
1920x1080@60/layout rule restored the output. The operator then authorized and
ran `WORLDWALL_BIN=target/debug/worldwall tools/run_s08_topology.sh
--authorized`. The helper observed real `PrepareForSleep(true/false)` signals,
recorded `disabled=True` during sleep and `disabled=False` after resume, and
Worldwall recorded `suspend state=paused`, `resume state=pending
output-ready=true`, and `resume-presented generation=2` with
`scene-upload-count=1`. The first corrected run exposed a Python verification
quoting bug and a resume-time restore race; both were fixed before the passing
run. `sh -n tools/run_s08_topology.sh`, `python3 tools/check_docs.py`, and the
helper's no-authorization refusal path also passed.

## Results

| Criterion | Exact command or manual procedure | Expected | Observed | Outcome | Artifact |
| --- | --- | --- | --- | --- | --- |
| S08-AC1 | `cargo test --locked --all-targets --all-features`; three operator-authorized `systemctl suspend --no-wall` cycles while `target/debug/worldwall` ran; journal capture | Sleep pauses submission/deadlines; resume proceeds only with valid output configuration and one bootstrap | Pure scheduler/state tests passed. The live fixed-scene run recorded 3 `EnteringSleep`/3 `Resumed` pairs, 3 paused states, 3 valid resume presentations, no frame submissions while paused, and clean continued rendering | pass for tested single-output live path | `src/power.rs`, `src/scheduler.rs`, `/tmp/worldwall-s08-ac5.log` (SHA-256 `ad7a9ea2668f5be83346129907a8a639bd3798fb491646376ff3fc697a9184a1`), `/tmp/worldwall-s08-suspend-journal.log` (SHA-256 `0f7c2bd392b21a0a6fd7cff336237fead0efb41cc9f054477d9fd5737fd79d56`) |
| S08-AC2 | `power::tests::duplicate_sleep_and_resume_signals_are_coalesced`; `power::tests::sleep_invalidates_a_pending_resume_generation`; existing scene transaction stale-generation tests | Duplicate signals and stale work cannot cause extra resume selections/swaps | Passed: duplicate sleep/resume actions coalesced; a sleep invalidated an older resume token; existing transaction tests reject stale completions | pass for deterministic generation/coalescing | `src/power.rs`, `src/scene_registry.rs` |
| S08-AC3 | Fixed-scene three-cycle run; registry run with candidate PNGs corrupted before the second authorized suspend; offline Open-Meteo run forced through `127.0.0.1:9`; existing transaction tests | Same scene avoids redundant upload; failed candidate/stale weather retains a working scene | Fixed-scene run remained at one upload across 3 resumes. Registry run applied one changed scene, then rejected the corrupted candidate with `Invalid PNG signature`, retained the active scene, presented successfully, and stayed at upload count 2. Offline weather startup was explicit and resume reported bounded `RateLimited`; it did not reselect the scene | pass for tested same-scene/candidate/weather paths; GPU recovery not-run | `/tmp/worldwall-s08-ac5.log`, `/tmp/worldwall-s08-failed.log` (SHA-256 `15ea59c5123de08752f5670d18bafc16ca98e6fcef5af879bb30b38ca81c4cf6`), `/tmp/worldwall-s08-offline.log` (SHA-256 `c9e162becfb7c9d168f0922cd27e6a30673de67056ce088f7fec758eec4c1b1c`) |
| S08-AC4 | System-bus startup trace; `power::tests::missing_service_is_degraded_without_a_false_resume`; `busctl --system status org.freedesktop.login1` | Available service is subscribed; missing/disconnected service is visible; no false resume; manual selection seam remains usable | The assigned Linux host reported `systemd-logind` available and the client emitted `power-service state=available`. Synthetic missing-service handling reports degraded/manual-refresh state and does not enter resume | pass for adapter availability and deterministic missing-service policy; actual disconnect not-run | `src/power.rs`, `/tmp/worldwall-s08-startup-final.log` (SHA-256 `51f1f80816df4aa656330c87ebca0c8c1deaaf6da319d9d9e51feb5d1e572dec`) |
| S08-AC5 | Three operator-authorized suspend/wake cycles; registry candidate-failure cycle; offline-weather cycle; corrected topology helper; journal and trace review | Real multi-cycle suspend evidence covers the required resume/pause/failure behavior, with topology change during sleep and maintainer review | Three real cycles, offline-weather startup/resume, failed-candidate retention, and the corrected topology subcase passed their recorded checks. The topology run disabled non-selected `HDMI-A-1` during sleep, restored its exact mode/layout after resume, and Worldwall resumed the selected `HDMI-A-2` output with one scene upload. `simplysoup` reviewed the complete S08 evidence and approved the stage on 2026-09-22 | pass; accepted by `simplysoup` 2026-09-22 | `/tmp/worldwall-s08-topology-live.log` (SHA-256 `ec0c9f5f07a33592197f7b6fe9d605c862b321e674672bf410d4d29b7fc852dd`), `/tmp/worldwall-s08-topology-live-helper.log` (SHA-256 `2cc520e252d202adb3ec67ada9239ba54ca49876cf57bb56170167c8bcfbc774`), `/tmp/worldwall-s08-topology-live-helper.log.err` (SHA-256 `6ff3778d36b3ffee1ac8504d4807f0ea0feb13324ea49b86431baaf68d094e17`) |

## Validation

- `cargo fmt --all`: pass.
- `cargo test --locked --all-targets --all-features`: pass — 70 passed, 3 ignored GPU tests.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: pass.
- `cargo build --locked`: pass.
- `WORLDWALL_TEST_EXIT_AFTER_MS=2000 timeout 12s target/debug/worldwall`: pass — real Wayland/Vulkan startup, one scene upload, adapter availability trace, paced frames, and clean bounded shutdown.
- Three `systemctl suspend --no-wall` cycles: pass — system journal records suspend entry/exit for all three cycles and the Worldwall trace records matching logind signals.
- Failed-candidate registry run: pass — invalid candidate was rejected and the prior GPU scene remained present.
- Offline-weather run: pass for bounded startup/resume behavior — explicit Open-Meteo configuration used a loopback proxy and temporary cache root; no provider success or network availability was claimed.
- `WORLDWALL_BIN=target/debug/worldwall tools/run_s08_topology.sh --authorized`: pass — real sleep changed non-selected `HDMI-A-1` to `disabled=True`, resume restored `disabled=False` at 1920x1080@60 and position 2272x960, and Worldwall resumed selected `HDMI-A-2` with one scene upload. The helper used Hyprland Lua `eval`; the D-Bus monitor fallback warning was recorded and did not prevent the logind signal trace.
- `busctl --system status org.freedesktop.login1`: pass — target host exposes `systemd-logind` on the system bus. This does not by itself prove every signal payload shape.

## Limitations and failures

- Three live `PrepareForSleep(true/false)` cycles were completed. A large wall-clock gap, a process timeout, or a screenshot would not substitute for that evidence.
- The topology subcase covered disabling and restoring a non-selected output. The single-output implementation still exits when its selected output is removed, as required by the existing S01/S02 lifecycle; S09 owns surviving a zero-output/multi-output topology and therefore no unsupported recovery was invented here.
- Service disconnect was not run, and GPU recovery during resume was not run. These remain explicit limitations rather than passes.
- The D-Bus worker is intentionally outside the Wayland thread. Its bounded channel drops excess duplicate notifications rather than blocking rendering; the state machine still coalesces duplicates by phase and generation.
- The live logs are ephemeral local artifacts under `/tmp`: the startup log is `/tmp/worldwall-s08-startup-final.log` (SHA-256 `51f1f80816df4aa656330c87ebca0c8c1deaaf6da319d9d9e51feb5d1e572dec`), the fixed-cycle, failed-candidate, offline-weather, topology, helper, control, and journal hashes are recorded in the tables/commands above. Logs contain no weather coordinates or credentials.

## Review and acceptance

- Reviewer / date: `simplysoup` / 2026-09-22
- Decision: accepted; ADR-0007 and S08-AC1–AC5 reviewed
- Unmet criteria / stale dependent evidence: none; service disconnect and GPU recovery remain documented not-run limitations, not waived gates
- Next owner / action: proceed to the planned S09 multi-output and hotplug stage; preserve the S08 traces as regression evidence.
