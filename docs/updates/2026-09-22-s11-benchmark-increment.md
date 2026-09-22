# 2026-09-22 — S11 benchmark execution increment

## Scope and ownership

- Stage / requirements: S11; S11-AC3, WW-05, WW-07, WW-09, WW-12.
- Owner: Pi coding agent. Approval authority: `simplysoup`.
- Approved inputs: [S11 performance matrix and budget proposal](2026-09-22-s11-performance-budget-proposal.md), approved 2026-09-22.
- Kind: assigned-session live benchmark on the approved Hyprland/Vulkan host.
- Raw artifact root: `/home/soup/.local/state/worldwall/s11-performance-20260922/`.

This increment records the first full 15/30 FPS dual-output animated runs,
the idle GPU baseline, one single-output animated run, and one interrupted
invalid static run. It does not accept S11 or silently revise a failed budget.

## Artifact identity and environment

- Git revision: `96425ac7cf4f2651df5669902dba704ab6f080e7`
- Release Worldwall SHA-256: `a75cbb61260a21cb3cf8df791b38a2bef1427f29b571bcd9e363a79670596212`
- Release `worldwallctl` SHA-256: `ea358c3415b40df3d467667cd9e3485db0ceb2df2d31231f3c5292b1d313e8e6`
- Toolchain: `rustc 1.94.0 (4a4ef493e 2026-03-02)`
- Environment record: `environment.txt`, SHA-256
  `6aae0d49238e7224c36928575358922912b9f9cc579f75271488f9250d3dbf4f`
- Scene input: local V29 package; input hashes are retained in
  `input-hashes.txt`. Weather was unset/disabled for every Worldwall run.
- The release binary was run from the current working tree whose changes are
  documentation/tooling only; clean-checkout release identity remains open for
  S11-AC1.

The output topology was two enabled 1920×1080 scale-1 transform-0 outputs for
B0, B7, and B8. B4 disabled `HDMI-A-1` for the run and restored it to its
recorded 1920×1080@60, position `2272x960`, scale 1, transform 0 state; the
pre/disabled/restored snapshots are retained without monitor serials in the
benchmark interpretation.

## Procedure

For B7, B8, and B4, the release binary was started with `stdbuf`, allowed to
present, warmed for 60 seconds, then sampled once per second for 900 seconds.
The raw CSV contains process CPU/RSS and `nvidia-smi` GPU utilization, board
power, and VRAM values. The process was externally terminated after sampling;
these are resource samples, not clean-release teardown evidence.

B0 collected 960 one-second GPU samples with Worldwall stopped. B5 was
interrupted after 584 samples and did not have the required 60-second warmup;
it is retained as an invalid partial artifact and is not a result.

## Results

| Workload | Procedure/artifact | Key observed steady-state values | Outcome |
| --- | --- | --- | --- |
| B0 idle baseline | `B0-idle-baseline-gpu.csv` (960 samples) | GPU utilization mean/p95 34.494/43%; board power 37.795/54.050 W; VRAM 932.745 MiB mean | baseline recorded |
| B7 dual animated 15 FPS | `B7-dual-animated-15.csv`, 900 samples; log SHA-256 `d68d20f899df2c90af4c034fd8ba18dd6139bdb226fd13be1e1be46fdd01a54d` | CPU 1.254/1.500%; RSS 241,860 KiB p95; GPU utilization 31.779/47%; board power 43.155/55.570 W; VRAM 1,338 MiB p95 | partial pass: CPU/RSS/GPU/pacing/upload/network invariants within approved proposal |
| B8 dual animated 30 FPS | `B8-dual-animated-30.csv`, 900 samples; log SHA-256 `e44f8670047dd8f74b6dc7bb875b1b309e72c240f62c389a6cd4c468b228ee92` | CPU 2.092/2.300%; RSS 242,148 KiB p95; GPU utilization 31.540/48%; board power 50.216/58.970 W; VRAM 1,424 MiB p95 | partial pass: CPU/RSS/GPU/pacing/upload/network invariants within approved proposal |
| B4 single animated 30 FPS | `B4-single-animated-30.csv`, 900 samples; log SHA-256 `fd751fb9d7d51ae10e38c03090fe7b13606754125564b8699a961ba4dfe1eb53` | CPU 1.640/1.800%; RSS 241,480 KiB p95; whole-GPU utilization 32.946/44%; board power 34.541/54.090 W; VRAM 1,290 MiB p95 | **not-comparable for GPU-utilization budget; rerun required with per-process telemetry** |
| B5 dual static 15 FPS | `B5-dual-static-15.csv` / log retained; 584 samples, no warmup | Static package intentionally omitted `animation.toml`; run was interrupted before the required procedure | not-run / invalid partial |

For B7 and B8, each output produced animation-frame phase deltas at the target
interval, `shared-scene-uploads=1` throughout, and seven V29 animation groups
were present. B7 had 14,851 frame records per output; B8 had approximately
29,5xx per output. B4 had one output and `shared-scene-uploads=1`. No weather
or HTTP request lines appeared in these logs.

The B4 30 FPS phase delta mean was approximately 33.419 ms with p95 33.387 ms
for its single output. B7 dual 15 FPS phase delta mean was approximately
66.706 ms with p95 66.727 ms. B8 dual 30 FPS phase delta means were
approximately 33.404 ms and 33.388 ms, with p95 33.387 ms. Occasional delayed
intervals were retained in the raw logs; no catch-up burst was inferred.

## Failure and interpretation

The first B4 run used whole-GPU `nvidia-smi` utilization and observed
32.946% mean against the proposed single-output limit of 25%. B0's idle
baseline independently observed 34.494%, so the counter is not attributable to
Worldwall and B4 is not comparable to the approved per-workload budget. This
measurement-method failure is recorded in
[RB-0002](../roadblocks/RB-0002-gpu-utilization-budget.md); the threshold was
not relaxed. The approved next experiment uses available per-process
`nvidia-smi pmon` telemetry.

B5's absence of visible animation was expected: it was the intentionally static
workload and its temporary package omitted animation metadata. The animated B7,
B8, and B4 runs used the V29 package with seven `blend_shift` groups and showed
monotonic animation-uniform updates, but trace evidence alone is not a color
readback or screenshot claim.

## Gate status and next action

- S11-AC3: **open**. B7/B8 satisfy their measured resource/pacing portions;
  B4's GPU-utilization measurement is not comparable and requires a pmon-backed
  rerun; most matrix workloads remain not-run.
- S11-AC1: open; the benchmark used an identified release binary, but the
  current working tree is dirty and clean-checkout package evidence is not yet
  recorded.
- S11-AC2, AC4, AC5: open and not addressed by this increment.
- Next safe action: rerun B4 and the completed dual-output cases with
  per-process pmon telemetry without changing the approved thresholds. Then
  rerun/complete the remaining workloads, including a proper B5 warmup/sample
  and the single-output 15 FPS cases.

## Review and acceptance

- Reviewer / date: not yet reviewed
- Decision: benchmark increment recorded; no S11 acceptance
- Unmet criteria: S11-AC1, S11-AC2, S11-AC3, S11-AC4, S11-AC5
