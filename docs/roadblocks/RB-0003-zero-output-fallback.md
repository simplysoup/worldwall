# RB-0003 — assigned compositor creates a fallback output

- State: resolved (limitation recorded)
- Opened / last updated: 2026-09-22
- Stage / blocked criteria: S11-AC3 B9 zero-output workload
- Owner / escalation target: Pi coding agent / `simplysoup` for matrix disposition
- Kind / impact: environment; normal — zero-output sampling was not
  established on this assigned compositor
- Next check: none; resolved by the approved B9 disposition. Reopen only if a
  valid no-output host procedure is later approved

## Expected versus observed

The approved B9 workload requires a resident Worldwall process with zero
outputs and zero steady-state frame submissions. The assigned Hyprland
procedure used:

```sh
hyprctl eval 'hl.monitor({ output = "HDMI-A-1", disabled = true })'
hyprctl eval 'hl.monitor({ output = "HDMI-A-2", disabled = true })'
```

The compositor disabled both physical outputs but created a `FALLBACK` output.
Worldwall attached a layer to that fallback output, so this was not a valid
zero-output workload. The raw topology snapshot and short trace are retained
under `/home/soup/.local/state/worldwall/s11-performance-20260922/` as
`B9-zero-output-pmon-1s-output-disabled.json` and
`B9-zero-output-pmon-1s.log`. The helper restored both physical outputs in its
exit trap, and the post-run topology showed both enabled.

## Diagnosis and attempts

| Hypothesis | Experiment | Result / evidence | Conclusion |
| --- | --- | --- | --- |
| Disabling both physical outputs yields a zero-output compositor state | Disable HDMI-A-1 and HDMI-A-2 during a live Worldwall run | Hyprland reported a new enabled `FALLBACK` output and Worldwall continued presenting on it | rejected |

No 900-second B9 sample was collected. No claim is made about zero-output
behavior on this compositor.

## Workaround and escalation

Keep B9 `not-run` rather than treating the fallback output as zero outputs.
Independent B1–B8 results and existing S09 zero-output/reconnect evidence are
not invalidated. A nested compositor or an explicitly authorized output-removal
procedure may provide a separate B9 environment; it must prove that no
fallback output receives the Worldwall layer.

## Resolution evidence

Resolved 2026-09-22 by the maintainer-approved B9 disposition in the [S11
result-review disposition](../updates/2026-09-22-s11-result-review-disposition.md):
B9 is `not-run` on this host with this recorded environment limitation,
matching the fallback the approved budget proposal defines. The S11-AC3
criterion text is unchanged; S09's zero-output/reconnect traces remain
directly recorded evidence and are not substituted for the B9 workload.

## Related records

- [S11 benchmark increment](../updates/2026-09-22-s11-benchmark-increment.md)
- [S11 performance budget proposal](../updates/2026-09-22-s11-performance-budget-proposal.md)
- [S09 multi-output evidence](../updates/2026-09-22-s09-multi-output.md)
