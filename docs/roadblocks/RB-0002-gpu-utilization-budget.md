# RB-0002 — S11 GPU utilization counter is not process-attributable

- State: resolved
- Opened / last updated: 2026-09-22
- Stage / blocked criteria: S11-AC3 GPU-utilization metric
- Owner / escalation target: Pi coding agent / maintainer `simplysoup`
- Kind / impact: measurement method; high — the first single-output result
  cannot be compared to the approved per-workload GPU budget
- Next check: retain the pmon-backed results with the S11 benchmark evidence;
  no threshold change is required

## Expected versus observed

The approved S11 proposal sets GPU-utilization budgets of ≤25% mean / ≤50%
p95 for one output and ≤40% mean / ≤65% p95 for two outputs. It requires
process/board telemetry where available.

The first B4 single-output animated 30 FPS run used
`nvidia-smi --query-gpu=utilization.gpu,...`, which reports whole-GPU
utilization. It observed 32.946% mean and 44% p95, exceeding the proposed
single-output mean limit. The paired B0 idle-compositor baseline independently
observed 34.494% mean and 43% p95, already above the single-output limit. The
result therefore cannot attribute utilization to Worldwall or establish a
Worldwall budget failure.

The raw B4 CSV and log are retained under
`/home/soup/.local/state/worldwall/s11-performance-20260922/` with hashes in
the linked benchmark update.

## Diagnosis and attempts

| Hypothesis | Experiment | Result / evidence | Conclusion |
| --- | --- | --- | --- |
| Whole-GPU query is an attributable Worldwall utilization metric | B0 idle baseline and B4 single-output run using `nvidia-smi --query-gpu=utilization.gpu` | B0 mean 34.494%; B4 mean 32.946%; other desktop GPU work is present | rejected |
| The assigned driver exposes a process-level alternative | `nvidia-smi pmon -c 1 -s um` on the assigned host | Command is available and reports per-PID SM, memory, and framebuffer columns; current sample lists Hyprland and other processes | confirmed as a viable next experiment; Worldwall-specific run pending |

No threshold has been relaxed or changed. B4 remains `not-comparable` for the
approved GPU-utilization criterion until rerun with process-level telemetry;
its CPU, RSS, pacing, upload, and network observations remain retained
separately.

## Workaround and escalation

Use `nvidia-smi pmon -s um` at the approved one-second sampling cadence and
record the Worldwall PID's SM utilization, memory utilization, and framebuffer
allocation. Keep the whole-board power query for the approved paired-baseline
power metric. If pmon omits the process or reports no usable value during a
required sample, record GPU utilization as `unavailable`; do not substitute
whole-GPU utilization or zero.

The safe independent work is to rerun B4 and the completed dual-output cases
with the corrected counter, then continue workloads whose metrics are
attributable. Any change to the approved threshold requires a separate review;
this record does not authorize one.

## Resolution evidence

Resolved by the pmon-backed B1–B8 reruns in
[the S11 benchmark matrix update](../updates/2026-09-22-s11-benchmark-pmon-reruns.md).
The Worldwall PID was present in all 900 samples for each rerun; active-window
SM means and p95 values stayed within the unchanged approved limits. Reviewer
/ date: `simplysoup` / 2026-09-22 approval of the measurement proposal.

## Related records

- [S11 performance budget proposal](../updates/2026-09-22-s11-performance-budget-proposal.md)
- [S11 benchmark increment](../updates/2026-09-22-s11-benchmark-increment.md)
- [S11 pmon-backed benchmark matrix](../updates/2026-09-22-s11-benchmark-pmon-reruns.md)
- [RB-0001 benchmark budgets](RB-0001-s11-benchmark-budgets.md)
