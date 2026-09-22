# 2026-09-22 — S11 pmon-backed benchmark matrix reruns

## Scope and decision

- Stage / requirements: S11; S11-AC3, WW-05, WW-07, WW-09, WW-12.
- Approval: `simplysoup` approved the matrix and thresholds on 2026-09-22.
- Host: assigned Hyprland/Vulkan session recorded in the [budget proposal](2026-09-22-s11-performance-budget-proposal.md).
- Raw root: `/home/soup/.local/state/worldwall/s11-performance-20260922/`.

This update supersedes the whole-GPU utilization interpretation from the first
benchmark increment for GPU-utilization purposes. It does not erase that
failed measurement-method attempt; it records the corrected per-process rerun.

## Corrected measurement method

The first B4 attempt queried whole-GPU utilization. Because the approved
proposal requires process/board telemetry where available, B1–B8 were rerun
with `nvidia-smi pmon -c 1 -s um`. The Worldwall PID was present in all 900
samples for every corrected run. A `-` SM value means the process had no
measured SM activity in that one-second pmon window; the full-window metric
therefore treats it as zero, while the active-window mean/p95 below is a
conservative upper-bound view of the numeric samples. Raw pmon output is
retained; no unavailable value was changed to a fabricated nonzero result.

Each workload used a 60-second warmup and 900 one-second samples. B1–B4 used
one enabled 1920×1080 output after temporarily disabling and restoring
`HDMI-A-1`; B5–B8 used both enabled outputs. B1/B2/B5/B6 used the temporary
static V29 package without `animation.toml`; B3/B4/B7/B8 used the V29 package
with seven `blend_shift` groups. Weather was disabled and no network request
was made.

## Results

The table gives process CPU mean/p95, RSS p95, process SM active-window
mean/p95, framebuffer allocation p95, and pacing phase-delta p95. The approved
limits are single-output CPU ≤6% at 15 FPS / ≤11% at 30 FPS, dual-output CPU
≤10% / ≤16%, RSS ≤256/384 MiB, process SM ≤25%/40% mean and ≤50%/65% p95,
and VRAM ≤1,536/2,048 MiB.

| Workload | CPU mean/p95 | RSS p95 | pmon SM active mean/p95 | pmon FB p95 | Pacing p95 | Result |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| B1 single static 15 | 1.119/1.200% | 246,020 KiB | 21.692/39% (26 active) | 227 MiB | 66.741 ms | pass for measured criteria |
| B2 single static 30 | 1.711/1.800% | 245,856 KiB | 16.027/33% (74 active) | 227 MiB | 33.387 ms | pass for measured criteria |
| B3 single animated 15 | 1.055/1.200% | 246,240 KiB | 18.722/30% (36 active) | 227 MiB | 66.742 ms | pass for measured criteria |
| B4 single animated 30 | 1.797/1.900% | 241,252 KiB | 24.282/39% (71 active) | 227 MiB | 33.387 ms | pass for measured criteria |
| B5 dual static 15 | 1.330/1.500% | 242,428 KiB | 12.829/28% (41 active) | 259 MiB | 66.727 ms | pass for measured criteria |
| B6 dual static 30 | 2.189/2.400% | 242,360 KiB | 17.337/30% (92 active) | 259 MiB | 33.387 ms | pass for measured criteria |
| B7 dual animated 15 | 1.317/1.500% | 242,364 KiB | 16.848/27% (46 active) | 259 MiB | 66.742 ms | pass for measured criteria |
| B8 dual animated 30 | 2.243/2.400% | 242,496 KiB | 21.988/34% (85 active) | 259 MiB | 33.387 ms | pass for measured criteria |

Full-window zero-filled pmon SM means/p95 were, respectively: B1
0.627/0%, B2 1.318/11%, B3 0.749/0%, B4 1.916/20%, B5 0.584/0%, B6
1.772/18%, B7 0.861/4%, and B8 2.077/21%. All are below the approved
limits. Process framebuffer allocation remained 227 MiB for one output and
259 MiB for two outputs; these are within the approved VRAM limits. Whole-board
power was retained as host-level paired-baseline data, not as per-process
power: the B0 idle baseline had mean/p95 37.795/54.050 W, while the corrected
runs ranged from 42.079 to 55.790 W mean. It does not establish a per-process
power claim.

Every corrected run retained `shared-scene-uploads=1`. Animated runs showed
monotonic animation-uniform updates and target phase deltas; static runs
intentionally had no animation metadata. No weather or HTTP request lines were
present.

## Artifact hashes

For each workload, the CSV, trace log, and raw pmon stream are retained:

```text
B1 csv 2850d9116b3b139b5b26d5c0853e032d20d965ddefc6c351a8bbb2e938c970aa
B1 log e91ee39ccf012c114dce137683d8e2e0477a4f44eca5853648aec1e0c1375b2e
B1 pmon 96cff577aad89d01669df9e7f3c1e70ddbf3a9c022c262128b1ff24c2484aa03
B2 csv 8d4f85ffd47ed61fab29386d7148dca98576e74ab81071217d964ca5605d9cf3
B2 log 9a0d0f5502a4629ae6ca059ea3f81fa74d49eb0b19b36752e60bbf8d16c0d3e1
B2 pmon 84c773f49b498abbc58cd4b0b7cfc72d7852cf5b7da8a231a358f4cda7b32387
B3 csv f7733b10364aef0fed9cb43c3c3205fc142f5dc95ecbb0f3d02760e1683d967e
B3 log 8dc911c7b443d382978f78201ac994dd5f0867550dc4c00d2b8d895362a8e4c0
B3 pmon 7045160e36d038c7696ea175eb45cd8f440b144c664712d80538967e1fad340d
B4 csv 74c2ca6c47c26a59635ffc35171eca4d6734f2fdd568582b0195ac5a2acd8020
B4 log 6a0f856d9cb69b0e88e50e16fabfecede6c993d33e6335ef755520e8d7c285b9
B4 pmon f8d4205e9d7d0a146d2d8d6f06dc981b1c74b0d55eaa7132ca3c21c2a39a0e1
B5 csv 1e6b8584cfd37064df6ea1042a1003d8c06ff9b15f673b70ce95f0373215cb5b
B5 log edab52f974428c388e0488aeba190d1d23eeb47f8738bef3bb5b57aa6094ba7e
B6 csv 98572d4bd09f56653b432f50bf60ec61983460556d191d3b4d355158ffacbd49
B6 log a8e642707f6671eae29f3ae15bae86c19114e181dce85a74c2038f90fe61fb03
B6 pmon 266f8b622e85e495838b1da7673423284f74428ea1446803f0708246bafeb9d7
B7 csv 2f02b29f000c55a8dc9d5c4a349136b94250023ebcf093c9aadad8c7e204056a
B7 log 1f7c2254853c007789972762c68171a6633e619535c4c6178aa3e9cf2c079014
B7 pmon e767b69ea7466445f98ec9b0049f7f5bf0d0bfcbd4d8c8d4f47e186ecda458cf
B8 csv 4171e168545fc6d5633736e689c6f90a4b5435aab9e83c5d1b314bd3f7534766
B8 log 0edad15a2f8c0b765c3762ece22ddabfa0e95e331f0fc16b08ffec0936c8f8e3
B8 pmon 258295f3aaa40d8a2d0b67a7346db95a0f8a217cb0879c4bda90e8bfa97ef4ba
```

The exact B5 pmon hash is:

```text
B5 pmon 16a881f702cb83d8de1a44740c1ec64b0990c1fed4c45b76fbe8ff608dca7052
```

## Remaining matrix limitations

The B9 no-output attempt disabled both physical outputs but Hyprland created an
enabled `FALLBACK` output and Worldwall presented there. It is recorded in
[RB-0003](../roadblocks/RB-0003-zero-output-fallback.md) and is not a zero-output
pass. B10 suspend sampling was not run in this increment. Existing S08
suspend/resume evidence is not silently substituted for this S11 resource
workload.

The initial whole-GPU B4 attempt and the interrupted/non-warmup B5 attempt are
retained in the earlier [benchmark increment](2026-09-22-s11-benchmark-increment.md)
as failed methodology/invalid partial artifacts, not acceptance results.

## Gate status

- S11-AC3: partial. B1–B8 resource/pacing/upload/network results satisfy the
  approved measured criteria; B9 is blocked by the fallback output and B10 is
  not-run. Whole-board power remains host-level observational data.
- S11-AC1, AC2, AC4, AC5: open.
- RB-0002: resolved by this pmon-backed rerun.
- RB-0003: open.
- Decision: no S11 acceptance claimed.

## Review and next action

- Reviewer / date: benchmark inputs approved by `simplysoup`; B1–B8 results
  reviewed and accepted with recorded dispositions in the [S11 result-review
  disposition](2026-09-22-s11-result-review-disposition.md), including the
  approved three-window sampling restatement and the per-process FB VRAM
  counter basis.
- Next owner/action: Pi coding agent to complete the remaining S11
  clean-checkout, recovery, documentation, and licensing gates.
