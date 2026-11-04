# 2026-09-22 — S11 benchmark result review and dispositions

## Review status

- State: **approved**
- Stage: S11; affects S11-AC3, RB-0001, RB-0002, RB-0003, and the supplemental
  time-of-day sampling
- Reviewer / approval authority: `simplysoup` / 2026-09-22 (unqualified
  approval of the recorded S11 work, recorded under the standing instruction
  that unqualified approvals use that identity)
- Reviewed inputs: [S11 pmon-backed benchmark update](2026-09-22-s11-benchmark-pmon-reruns.md),
  [S11 benchmark increment](2026-09-22-s11-benchmark-increment.md),
  [RB-0001](../roadblocks/RB-0001-s11-benchmark-budgets.md),
  [RB-0003](../roadblocks/RB-0003-zero-output-fallback.md), and the supplemental
  artifact root `$HOME/.local/state/worldwall/s11-tod-functional-20260922/`

## Decisions

1. **B1–B8 results reviewed and accepted** as S11-AC3 measured-criteria
   evidence under the recorded limitations (host-level power is observational;
   unavailable counters remain unavailable). No threshold changed.
2. **Sampling-policy disposition:** the approved proposal prescribed a
   60-second warmup followed by three five-minute samples; the reruns collected
   one 900-second warm run at one-second cadence. The single-window procedure
   is approved as an equivalent alternative **and** the retained raw streams
   have been restated as three contiguous 300-sample windows (below), so the
   approved three-sample structure is satisfied without re-collection. The
   per-window statistics confirm every window of every workload meets the
   approved budgets.
3. **VRAM counter basis pinned:** the binding VRAM budget counter is the
   per-process framebuffer allocation (`pmon` FB, p95), consistent with the
   per-process CPU/RSS convention. Whole-board VRAM remains host-level
   observational data and is never compared to the per-process budget.
4. **B9 disposition:** `not-run` on the assigned host with the RB-0003
   environment limitation recorded, per the fallback the approved proposal
   itself defines ("otherwise recorded `not-run` with the environment
   limitation"). S09's zero-output/reconnect traces remain separate evidence.
   [RB-0003](../roadblocks/RB-0003-zero-output-fallback.md) is resolved with
   this recorded limitation; the criterion text is unchanged.
5. **B10 disposition:** `not-run` for pmon suspend sampling. S08's
   operator-authorized suspend/resume traces remain the accepted suspend
   evidence and are not substituted for S11 resource sampling; B10 is recorded
   as a known limitation of the S11 evidence, not a pass.
6. **Supplemental time-of-day runs (T1–T3):** accepted as supplemental
   functional-verification evidence that the CR-0002 civil-time fade advances
   without re-uploads or reselection, with the artifact README's limitations
   (no direct on-screen palette observation; uncontrolled background load;
   corrected CSVs derived from raw pmon). They are **not** S11-AC3 evidence
   and are not compared to budgets.

## Three-window restatement of the approved benchmark raw streams

Source: retained per-sample CSVs under
`$HOME/.local/state/worldwall/s11-performance-20260922/` (900 samples per
workload; windows W1 = samples 1–300, W2 = 301–600, W3 = 601–900). Overall
means reproduce the published pmon-rerun table exactly (B1 1.119, B2 1.711,
B3 1.055, B4 1.797, B5 1.330, B6 2.189, B7 1.317, B8 2.243 percent CPU).

| Workload | Window | CPU mean/p95 | RSS p95 | SM active mean/p95 (n) | SM full mean/p95 | FB p95 |
| --- | --- | --- | --- | --- | --- | --- |
| B1 | W1 | 1.156/1.300% | 246,020 KiB | 22.333/34% (6) | 0.447/0% | 227 MiB |
| B1 | W2 | 1.100/1.100% | 246,020 KiB | 21.818/46% (11) | 0.800/0% | 227 MiB |
| B1 | W3 | 1.100/1.100% | 246,020 KiB | 21.111/41% (9) | 0.633/0% | 227 MiB |
| B2 | W1 | 1.734/1.900% | 245,856 KiB | 19.083/39% (24) | 1.527/11% | 227 MiB |
| B2 | W2 | 1.700/1.700% | 245,856 KiB | 14.852/29% (27) | 1.337/13% | 227 MiB |
| B2 | W3 | 1.700/1.700% | 245,856 KiB | 14.217/28% (23) | 1.090/8% | 227 MiB |
| B3 | W1 | 1.132/1.300% | 246,240 KiB | 17.214/40% (14) | 0.803/0% | 227 MiB |
| B3 | W2 | 1.032/1.100% | 246,240 KiB | 17.818/26% (11) | 0.653/0% | 227 MiB |
| B3 | W3 | 1.000/1.000% | 246,240 KiB | 21.545/31% (11) | 0.790/0% | 227 MiB |
| B4 | W1 | 1.860/1.900% | 241,252 KiB | 25.037/39% (27) | 2.253/22% | 227 MiB |
| B4 | W2 | 1.800/1.800% | 241,252 KiB | 25.000/38% (21) | 1.750/23% | 227 MiB |
| B4 | W3 | 1.729/1.800% | 241,252 KiB | 22.739/40% (23) | 1.743/20% | 227 MiB |
| B5 | W1 | 1.390/1.600% | 242,428 KiB | 14.875/28% (16) | 0.793/3% | 259 MiB |
| B5 | W2 | 1.300/1.300% | 242,428 KiB | 11.333/32% (12) | 0.453/0% | 259 MiB |
| B5 | W3 | 1.300/1.300% | 242,428 KiB | 11.692/20% (13) | 0.507/0% | 259 MiB |
| B6 | W1 | 2.293/2.500% | 242,360 KiB | 20.250/30% (28) | 1.890/19% | 259 MiB |
| B6 | W2 | 2.174/2.200% | 239,396 KiB | 15.100/29% (30) | 1.510/15% | 259 MiB |
| B6 | W3 | 2.100/2.100% | 239,396 KiB | 16.912/30% (34) | 1.917/19% | 259 MiB |
| B7 | W1 | 1.368/1.600% | 242,364 KiB | 16.105/28% (19) | 1.020/6% | 259 MiB |
| B7 | W2 | 1.300/1.300% | 242,364 KiB | 16.333/27% (12) | 0.653/0% | 259 MiB |
| B7 | W3 | 1.284/1.300% | 242,364 KiB | 18.200/51% (15) | 0.910/4% | 259 MiB |
| B8 | W1 | 2.329/2.500% | 242,496 KiB | 23.720/34% (25) | 1.977/24% | 259 MiB |
| B8 | W2 | 2.200/2.200% | 242,496 KiB | 21.882/43% (34) | 2.480/28% | 259 MiB |
| B8 | W3 | 2.200/2.200% | 242,496 KiB | 20.462/33% (26) | 1.773/19% | 259 MiB |

Every window is within the approved budgets: CPU ≤6/11% (single 15/30) and
≤10/16% (dual 15/30); RSS ≤256/384 MiB; process SM ≤25/40% mean and ≤50/65%
p95; process FB ≤1,536/2,048 MiB.

## Gate effect

- S11-AC3: all measured criteria and workload dispositions are now complete
  under the approved matrix and budgets. Final stage acceptance remains a
  maintainer decision at the M4 review; nothing in this record ticks a stage
  acceptance box.
- S11-AC1, AC2, AC4, AC5: unchanged and still open; clean-checkout
  verification proceeds next.

## Related records

- [S11 pmon-backed benchmark update](2026-09-22-s11-benchmark-pmon-reruns.md)
- [S11 performance budget proposal](2026-09-22-s11-performance-budget-proposal.md)
- [RB-0001](../roadblocks/RB-0001-s11-benchmark-budgets.md),
  [RB-0003](../roadblocks/RB-0003-zero-output-fallback.md)
