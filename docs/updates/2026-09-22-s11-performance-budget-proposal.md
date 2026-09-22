# 2026-09-22 — S11 performance matrix and budget proposal

## Review status

- State: **approved for S11 benchmarking**
- Stage: S11; affects S11 entry and S11-AC3 / WW-05, WW-07, WW-09, WW-12
- Proposer: Pi coding agent
- Reviewer / approval authority: `simplysoup` / 2026-09-22
- Decision: approve the matrix and thresholds as written for the assigned host;
  unavailable counters remain `unavailable`, not zero
- Related blocker: [RB-0001](../roadblocks/RB-0001-s11-benchmark-budgets.md)

This record supplies a concrete matrix and thresholds for review. It does not
advance S11, claim a benchmark result, or change the delivery contract.

## Observed host for review

The following commands were run from the assigned session on 2026-09-22:

```sh
uname -srvm
grep -E '^(PRETTY_NAME|VERSION_ID)=' /etc/os-release
hyprctl version
nvidia-smi --query-gpu=name,driver_version --format=csv,noheader
vulkaninfo --summary
hyprctl monitors -j
```

Observed values, with monitor serials and other identifying details omitted:

| Field | Proposed matrix value |
| --- | --- |
| OS | Arch Linux |
| Kernel | Linux 7.2.4-zen2-1-zen, x86_64 |
| Compositor | Hyprland 0.56.2, commit `efb50993780079460b0cbed1363e2166a2de1d9f` |
| GPU | NVIDIA GeForce RTX 3070 Ti |
| NVIDIA driver | 615.71.09 |
| Vulkan instance | 1.4.357 |
| Vulkan device API | 1.4.351 |
| Wayland outputs | `HDMI-A-1` and `HDMI-A-2`, each 1920×1080 at 60 Hz, scale 1, transform 0, enabled |
| Worldwall toolchain | Rust/Cargo 1.94.0; locked wgpu 30.0.1 dependency |

The output state is a live observation, not a universal hardware-support
claim. The benchmark artifact must repeat the version and output inspection
immediately before sampling.

## Proposed workloads

All workloads use the same validated local scene package as the S04 baseline,
with its path and SHA-256 recorded in the raw artifact. Weather is disabled and
no external network request is permitted during these workloads. The same
scene is used on every active output; per-output scene selection is not part of
the contract.

| ID | Outputs | Scene/effect | Target |
| --- | --- | --- | --- |
| B0 | compositor baseline, Worldwall stopped | no Worldwall process | idle reference |
| B1 | one | static indexed scene | 15 FPS |
| B2 | one | static indexed scene | 30 FPS |
| B3 | one | animated indexed scene | 15 FPS |
| B4 | one | animated indexed scene | 30 FPS |
| B5 | two | static indexed scene | 15 FPS |
| B6 | two | static indexed scene | 30 FPS |
| B7 | two | animated indexed scene | 15 FPS |
| B8 | two | animated indexed scene | 30 FPS |
| B9 | zero, if the assigned live procedure can safely remove both outputs | resident process | no frame submissions |
| B10 | suspended, if supported by the assigned live procedure | resident process | paused deadlines and no frame submissions |

B9 and B10 are required workload cases where the host procedure supports them;
otherwise they are recorded `not-run` with the environment limitation. They
cannot be replaced by a headless or mock pass.

For each B1–B8, collect a 60-second warmup followed by three five-minute
samples. Keep startup, initial scene upload, reconfigure, and recovery events
outside the steady-state statistics. B0 uses the same output topology and
background workload as its paired Worldwall run.

## Proposed acceptance budgets

These are release limits, not predictions. They are intentionally above the
accepted observational S04 single-output baseline while preserving the
no-busy-loop and shared-resource requirements. A maintainer may approve,
reject, or revise them before measurement.

### Process and GPU resources

Measurements use one-core CPU percentages from `ps`/`pidstat`, RSS from the
same process, and NVIDIA process/board telemetry where the counter is
available. Report steady-state mean and p95; exclude startup and upload
windows as described above.

| Workload class | CPU mean | CPU p95 | RSS p95 | VRAM p95 |
| --- | ---: | ---: | ---: | ---: |
| B1/B3, one output at 15 FPS | ≤ 6% of one core | ≤ 12% | ≤ 256 MiB | ≤ 1,536 MiB |
| B2/B4, one output at 30 FPS | ≤ 11% of one core | ≤ 20% | ≤ 256 MiB | ≤ 1,536 MiB |
| B5/B7, two outputs at 15 FPS | ≤ 10% of one core | ≤ 18% | ≤ 384 MiB | ≤ 2,048 MiB |
| B6/B8, two outputs at 30 FPS | ≤ 16% of one core | ≤ 28% | ≤ 384 MiB | ≤ 2,048 MiB |

The S04 baseline observed 4.8–5.3% CPU at 15 FPS, 8.6–8.9% at 30 FPS,
216,084–220,844 KiB RSS maxima, 8.9–14.2% GPU utilization, approximately
30.58–31.18 W board power, and 1,186 MiB VRAM for its four single-output
workloads. Those observations are the comparison baseline, not an acceptance
waiver.

Proposed GPU utilization limits are ≤ 25% mean / ≤ 50% p95 for one output and
≤ 40% mean / ≤ 65% p95 for two outputs. GPU power is evaluated as whole-board
power relative to the paired B0 baseline, not as a claim of per-process power:

| Workload class | Mean board power over paired B0 | p95 board power over paired B0 |
| --- | ---: | ---: |
| one output, static | ≤ B0 + 5 W | ≤ B0 + 10 W |
| one output, animated | ≤ B0 + 10 W | ≤ B0 + 20 W |
| two outputs, static | ≤ B0 + 8 W | ≤ B0 + 15 W |
| two outputs, animated | ≤ B0 + 15 W | ≤ B0 + 25 W |

If a required counter is unavailable, record `unavailable` rather than zero;
that workload fails the corresponding release metric until the maintainer
approves an alternative. The proposed alternative for VRAM, if NVIDIA
per-process accounting is unavailable, is application allocation counters plus
an explicit `VRAM unavailable` limitation; this alternative does not claim a
VRAM budget was met. Power has no proposed substitute on this host because
`nvidia-smi` is currently available and a board-level baseline is measurable.

### Pacing and resource invariants

For target interval `T` (66.666 ms at 15 FPS and 33.333 ms at 30 FPS), after
bootstrap/reconfigure/recovery frames:

- zero steady-state per-output submission intervals may be less than `T - 2`
  ms;
- the steady-state submission rate must not exceed the configured ceiling by
  more than 1% (15.15 or 30.30 FPS), with delayed compositor callbacks
  reported rather than hidden;
- p95 positive interval lateness should be ≤ 20 ms at 15 FPS and ≤ 16 ms at
  30 FPS when the output remains enabled and the compositor is dispatching;
- B9/B10 must produce zero steady-state frame submissions while no output or
  paused;
- scene upload count must remain constant through every steady-state sample;
  no normal animation frame may upload the scene textures;
- selection generation must not change during clock ticks or weather results;
- the default benchmark configuration must produce zero network requests.

A separate opt-in weather cadence check may use a fake/local provider only:
normal polling must be no more frequent than one request per 900 seconds, with
at most one coalesced request for a resume event and no overlapping request.
No live provider request is authorized by this proposal.

## Raw artifact and decision procedure

Proposed output root:

```text
/home/soup/.local/state/worldwall/s11-performance-20260922/
```

Retain the exact command, binary/package SHA-256, tool versions, output
inspection, environment variables with secrets omitted, raw `ps`/`pidstat`,
`nvidia-smi`, trace logs, per-output frame timestamps, upload/selection
counters, and a summary comparing each result to the tables above. The package
must be built from a clean checkout before release claims.

Approval recorded above. Benchmark results remain pending execution and will
be recorded separately; this approval does not itself pass S11-AC3 or claim
hardware results.
