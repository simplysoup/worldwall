# 2026-09-22 — S11-AC2 recovery matrix on the release candidate

## Scope

- Stage / requirements: S11; S11-AC2, WW-01–WW-12.
- Candidate: release binary `worldwall` SHA-256 `a75cbb61…70596212` /
  `worldwallctl` SHA-256 `ea358c34…1313e8e6`, identical to the packaged and
  benchmark-sampled binaries (package digest `45ed1507…4976906d`,
  source revision `906aab8…`).
- Executed on the assigned Hyprland/Vulkan session. No weather configured (no
  network), no compositor/session configuration changes, no output topology
  changes. Every executed row is hardware-observed unless marked otherwise.

## Matrix

| # | Required case | Procedure | Result | Evidence class |
| --- | --- | --- | --- | --- |
| 1 | Hostile/malformed local IPC: malformed and oversized frames, unsupported protocol version, stalled clients, 12 concurrent clients, rapid generation stress, stale socket path/permissions, diagnostic rate limiting | `run_s10_ipc.py --binary target/release/worldwall` → `s11-ac2-ipc-20260922T215931Z/` | pass; malformed/oversized/version errors rejected with structured `protocol_error`, service stayed resident, status responses bounded (`status_seconds` 0.010 s under stalled clients) | hardware-observed |
| 2 | Foreign-UID peer rejection | Not re-run this increment (no passwordless root on this host) | `not-run` here; mapped to the accepted S10 root-run evidence (`s10-ipc-live-root-*` records) | mapped; not substituted silently |
| 3 | Repeated startup/shutdown (5 cycles) | `run_s11_ac2_startup.py` C1–C5: launch, present, SIGTERM, bounded wait | pass; every cycle presented ~470 frames, kept `uploads=1`, exited on SIGTERM within the 5 s bound, no panic | hardware-observed |
| 4 | Invalid config (`fps = 60`) | `WORLDWALL_CONFIG` with out-of-range fps | pass; `config line 2: fps must be 15 or 30`, exit rc=1 | hardware-observed |
| 5 | Invalid assets: nonexistent scene dir | `WORLDWALL_SCENE_DIR=/nonexistent/...` | pass; `reading .../scene.png failed: No such file or directory`, exit rc=1 | hardware-observed |
| 6 | Invalid assets: corrupt PNG | Truncated `scene.png` (64 bytes) | pass; `scene PNG pixel decode failed: unexpected end of file`, exit rc=1 | hardware-observed |
| 7 | Invalid assets: malformed `animation.toml` (overlapping groups) | Two overlapping `blend_shift` ranges | pass; `animation groups overlap at palette index 4`, exit rc=1 | hardware-observed |
| 8 | Registry-path transitions, resident-scene retention on config reload | IPC runner rows `registry_path_transition`, `registry_disable_retains_scene` | pass; selection generation advanced, scenes changed (`v29-seascape--cloudy` → `v26-winter-forest`), resident scene retained with `reselected=false` when registry disabled | hardware-observed |
| 9 | Mixed outputs | IPC runner service ran with two outputs (`outputs` list of 2) throughout | pass | hardware-observed |
| 10 | No outputs | Not achievable on this host (Hyprland `FALLBACK` output) | `not-run` per the approved RB-0003 disposition; S09 zero-output/reconnect traces remain separate evidence | mapped |
| 11 | Suspend/resume | Not re-run on the release candidate: a live re-run suspends the operator's entire session and requires explicit coordination | mapped to accepted S08 operator-authorized three-cycle suspend/resume traces; recorded as not re-run, never as a pass | mapped |
| 12 | Offline startup / slow or corrupt weather | Live rows require network authorization; weather was disabled by design | `not-run` live; deterministic coverage referenced from S07 unit tests (request timeout, offline cancellation, corrupt cache recovery, error mapping) | simulated (pure tests), labeled |
| 13 | Device-lost / OOM injection | Not injectable on the release binary (debug-only fault seams) | no unresolved defect; the bounded surface-recreate retry path is code-reviewed and exercised by the S02 debug fault matrix; recorded `not-run` for release-binary injection | mapped/partial |
| 14 | Selection/color re-checks | Registry selection exercised live on the RC (row 8); color paths re-verified by `cargo test --locked` readback vectors | pass for selection; color evidence remains the accepted S03/S05/S06 readback records | mixed (live + pure) |
| 15 | `worldwallctl` control smoke on the RC | status / reload / next against a direct-package service | pass; `status` ok; `reload`/`next` return structured `selection_unavailable` — the documented bounded outcome without a validated registry selection | hardware-observed |

## Failure-path diagnostics quality

Every invalid-input row produced a single precise diagnostic line and exited
rc=1 immediately: no hangs, no retries, no partial resources, no panics
(`results.json` `_verdict.failures` is empty). Raw traces and JSON results are
retained under `$HOME/.local/state/worldwall/`:

- `s11-ac2-ipc-20260922T215931Z/` — `results.json` SHA-256 prefix
  `764984094aae4dcd`, `worldwall.log` prefix `d91fdc5db67e6b78`
- `s11-ac2-startup-20260922/` — `results.json` prefix `8ec0f8171fade27c`,
  per-run logs and the driver `run_s11_ac2_startup.py`

## Gate effect and limitations

- S11-AC2: executed rows pass with bounded outcomes; no critical lifetime,
  data-loss, privacy/security, or busy-loop defect was observed. Rows 2, 10,
  11, 12, and 13 are explicitly `not-run` or mapped as tabled above — they are
  limitations of this evidence, not passes, and remain visible for the M4
  review.
- S11-AC4 and AC5 remain open (packaged-layout doc verification; licensing/
  provider-terms review and the maintainer release decision).

## Review and next action

- Reviewer / date: implementation increment by Pi coding agent; maintainer
  review at the M4 gate.
- Next owner/action: Pi coding agent to verify the operator runbook against
  the packaged layout (AC4) and complete the licensing/provider-terms review
  (AC5).
