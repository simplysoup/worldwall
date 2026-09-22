# RB-0001 — S11 benchmark matrix and budgets are not approved

- State: resolved
- Opened / last updated: 2026-09-22
- Stage / blocked criteria: S11 entry condition; S11-AC3
- Owner / escalation target: Pi coding agent / maintainer `simplysoup`
- Kind / impact: decision; high — release benchmarking and S11-AC3 cannot proceed
- Next check: none; resolved with the maintainer result review. Reopen only
  if benchmark inputs change

## Expected versus observed

The S11 stage requires a named hardware/workload matrix and numeric CPU,
memory, pacing, GPU, power, and network budgets before release measurements.
The validation procedure also requires the CPU percent-of-one-core convention,
raw sampling duration/tools, and an approved alternative when a metric is
unavailable.

On 2026-09-22, the repository was searched with:

```sh
rg -n -i "approved (numeric )?(budget|limit)|numeric (release )?(budget|limit)|hardware/workload matrix|workload matrix|cpu budget|memory budget|rss budget|vram budget|power budget|network budget|frame[- ]pacing budget|release budget|S11-AC3|S11 benchmarking|maintainer.*budget|budget.*maintainer" . --glob '!target/**' --glob '!.git/**'
```

The result contained only the S11 requirements, the open decision-register
row, the S04 note that release limits belong to S11, and the S11 packaging
update. At that point no approved values, named workload matrix, or accepted
alternative metrics were found. The decision register still lists “Supported
scaling/device matrix and resource budgets” as `open`.

The maintainer subsequently directed that the currently observed assigned host
be recorded as the S11 matrix detail:

- compositor: Hyprland 0.56.2;
- Vulkan: 1.4.357;
- GPU: NVIDIA RTX 3070 Ti;
- outputs: two 1920×1080 outputs, scale 1, with no transform variation
  recorded in this matrix.

The kernel, NVIDIA driver version, OS release, and workload-specific output
selection are not recorded in the supplied host observation. They must be
captured in the benchmark artifact rather than inferred.

## Diagnosis and attempts

| Hypothesis | Experiment | Result / evidence | Conclusion |
| --- | --- | --- | --- |
| The approved matrix and thresholds are already recorded locally | Repository-wide search above, followed by inspection of `docs/decisions/README.md`, `docs/validation.md`, and `docs/stages/11-release-validation.md` | The required decision remains open; `docs/status.md` says numerical release budgets remain due | rejected |
| The supplied host detail is sufficient to unblock measurement | Record the supplied host observation in this roadblock | Host identity is now recorded, but numeric thresholds and approved unavailable-metric alternatives remain absent | rejected |

No benchmark process was started, so there is no failed measurement artifact to
preserve. The existing `tools/measure_s04.sh` is explicitly observational S04
baseline tooling and cannot supply S11 budgets or acceptance evidence.

## Workaround and escalation

Independent S11 work may continue on release packaging, operator
documentation, and bounded recovery tooling. A concrete proposed matrix and
budget is now available in the [S11 performance budget proposal](../updates/2026-09-22-s11-performance-budget-proposal.md).
The host details above are recorded, but do not collect or label release
performance results until the maintainer approves that proposal or supplies
specific replacements:

- OS/kernel and NVIDIA driver versions, plus the workload/output selection;
- workloads covering the required idle/static/animated 15/30 FPS, one/multiple
  outputs, and supported no-output/suspend cases;
- CPU convention and numeric CPU, RSS/VRAM, frame-pacing, GPU-utilization or
  power limits, plus approved alternatives for unavailable counters;
- network cadence limit and the raw-sampling policy (60-second warmup and three
  five-minute samples, or an explicitly approved unchanged alternative).

The safe workaround is to remain `not-run` for S11 benchmarking. It expires
when the maintainer decision is recorded; it does not relax, waive, or move the
S11 gate.

## Resolution evidence

The maintainer approved the named matrix and numeric budgets in the linked
proposal on 2026-09-22. The pmon-backed B1–B8 runs were collected with raw
artifacts retained, and the maintainer result review (including the approved
three-window sampling restatement) is recorded in the [S11 result-review
disposition](../updates/2026-09-22-s11-result-review-disposition.md). Resolved
2026-09-22.

## Related records

- [Supported scaling/device matrix and resource budgets](../decisions/README.md)
- [S11 release-package increment](../updates/2026-09-22-s11-release-packaging.md)
- [S11 performance budget proposal](../updates/2026-09-22-s11-performance-budget-proposal.md)
- [S11 stage](../stages/11-release-validation.md)
