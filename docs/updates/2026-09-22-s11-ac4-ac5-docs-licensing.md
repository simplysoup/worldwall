# 2026-09-22 — S11 packaged-layout operator verification (AC4) and licensing review (AC5)

## Scope

- Stage / requirements: S11; S11-AC4, S11-AC5, WW-12.
- Candidate: release package created from clean tree `7efa663f…`
  (`source_dirty=false`), artifact digest
  `37c3a3290263b5b29f261e55b4f87b31e625facf19349b8f5f618e011e8e09dc`,
  `bundled_artwork=[]`, 10 packaged inputs. The digest differs from the AC1
  record's `45ed1507…` solely because this increment added the provider-terms
  disclosure to `docs/release.md`, which is a packaged input; both binaries
  remain `a75cbb61…` / `ea358c34…`.
- Executed on the assigned Hyprland/Vulkan session.

## S11-AC4 — operator runbook verified on the packaged layout

Procedure: copied the package to an operator-controlled location
(`~/.local/share/worldwall-ac4-operator`), then followed
[the runbook](../release.md) literally from that directory.

| Runbook claim | Observed |
| --- | --- |
| Foreground launch from the package with `WORLDWALL_CONFIG` | launched; loaded the packaged `examples/worldwall.toml` (`fps=30`, registry `none`), synthetic-reference scene as documented when no scenes are supplied |
| `bin/worldwallctl status` | structured status: backend vulkan, 2 outputs, weather disabled |
| `bin/worldwallctl weather` reports cache state only, no networking | `state: disabled`, zero network requests (no provider configured) |
| `bin/worldwallctl reload-config` validates and does not reselect | `applied=true`, `reselected=false` |
| Stale same-user socket is removed safely | restarted over the leftover socket; bound and served status immediately; socket mode `0600` |
| No undocumented global changes | launch used only documented env/paths; no session/compositor changes; ran as the desktop user, never root |
| Bounded behavior | 812 presented frames across two runs; zero panic/fatal lines |

Raw launch log retained at
`/home/soup/.local/share/worldwall-ac4-operator/ac4-run.log`.

## S11-AC5 — license, provenance, and artifact identity

| Item | Status |
| --- | --- |
| Software license | MIT; `LICENSE` packaged, `license = "MIT"` in `Cargo.toml` |
| Bundled artwork | none; `bundled_artwork=[]` enforced by `package_release.py` and its tests; fixtures remain untracked |
| Operator-supplied art | runbook directs operators to verify provenance/license before use |
| Provider terms | verified from `open-meteo.com/en/terms` on 2026-09-22: free API is non-commercial only, ≤10,000 calls/day / 5,000/hour / 600/minute, data under CC-BY 4.0 (attribution required). Worldwall polls at most once per 900 seconds (~96 calls/day), disabled by default; the operator enabling it is the API consumer. Disclosure added to the runbook this increment. |
| Secrets | packaged example config contains no credentials; resolved-config records environment names, not values |
| Artifact identity | manifest records source revision, dirty state, per-file SHA-256, and the deterministic package digest; binaries identical to the benchmark-sampled and AC1/AC2-tested binaries |

## Status and limitations

- S11-AC4: packaged-layout verification executed and recorded; stage
  acceptance remains the maintainer's M4 decision.
- S11-AC5: licensing/provenance/artifact-identity review complete to this
  record's scope; the M4 acceptance decision itself remains open.
- The Open-Meteo terms summary reflects the live terms page on 2026-09-22 and
  links the source for the current text; it is not a legal substitute.

## Review and next action

- Reviewer / date: implementation increment by Pi coding agent; maintainer
  review at the M4 gate.
- Next owner/action: `simplysoup` M4 release review — all S11 gate evidence
  (AC1 clean-checkout, AC2 recovery matrix with recorded `not-run`/mapped
  rows, AC3 reviewed benchmarks and dispositions, AC4 packaged-layout
  verification, AC5 licensing review) is now recorded; publishing remains a
  separate explicitly authorized action.
