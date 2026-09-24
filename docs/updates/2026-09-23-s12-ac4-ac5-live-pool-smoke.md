# S12 AC4/AC5 authorized live pool-mode smoke run

**Stage:** S12 (post-M4) · **Criteria exercised:** S12-AC4 (transactional
schema-v5 pool consumption) and S12-AC5 (shared automatic-selection filter) in
a live runtime · **Requirement IDs:** WW-06, WW-08, WW-10 ·
**Authorization:** maintainer-authorized live pool-mode smoke run, 2026-09-23 ·
**Owner:** Pi coding agent.

Scope: one bounded live smoke of the runtime pool mode against a **nested, real
Hyprland instance** on the assigned host. The production wallpaper process,
its outputs, and all session configuration were left untouched (verified by the
retained outer-session capture and the untouched production socket). This is
implementation/smoke evidence, **not** gate acceptance, and it does not touch
the CR-0006 supplemental gates.

## Method

[`tools/run_s12_pool_smoke.sh`](../../tools/README.md) (new, re-runnable)
assembles a temporary smoke registry/library/sidecar from the private untracked
derived fixtures (clearly labelled local test material; never written back to
the repository), starts nested Hyprland through the preserved
`tools/nested_wayland_connect.c` pattern, and runs the real `worldwall` with:

- a **private `XDG_RUNTIME_DIR`** so the child's control socket is distinct
  from the production instance (every command uses `worldwallctl --socket`);
- an **absolute `WAYLAND_DISPLAY`** socket path (a name would otherwise resolve
  against the private runtime dir);
- schema-v5 `worldwall.toml` with `registry_dir` and `scene_pools_file`, and
  `[weather]` disabled throughout, so **no provider request is possible**.

Smoke packages carry wildcard tags so pool rules are the only availability
filter; assets are copied from the private V08/V19/VW3B derived packages and
the private V25HEAT capture package (license `unreviewed`, unpublished).

## Phase A — pool mode with unknown environment inputs (no location configured)

Sidecar: `smoke-a`/`smoke-b` (registry pool, unconstrained), `smoke-c` (library
pool, `weather_any = ["clear"]`), `canvascycle-v25heat` (library pool,
`enabled = false`). Live results, all asserted in the runner:

1. **Startup selected `smoke-b` (generation 1)** — only pool-eligible
   candidates were considered; the unknown-weather and disabled scenes stayed
   filtered. Startup trace: `scene-source=registry id=smoke-b trigger=startup`.
2. **`next` cycled the eligible set deterministically with wrap**
   (`smoke-b → smoke-a → smoke-b`, generations 2–3).
3. **Manual `scene <id>` is the explicit override**: `scene smoke-c` (blocked by
   an unknown-weather criterion) and `scene canvascycle-v25heat` (disabled
   scene) both applied (generations 4–5).
4. **Automatic paths return only to the eligible set**: `next` from an
   overridden scene selected `smoke-a` (generation 6), never the blocked pair.
5. **`reload-config` applied new pool/rule state atomically without selecting**:
   response contained `reselected: False`, and the resident scene and selection
   generation were unchanged (`smoke-a 6`) even though `smoke-a` became
   ineligible under the new rules.
6. **`next` with no eligible candidate is a no-op with a bounded reason**:
   `{"applied": false, "outcome": "noop", "reason": "no eligible scene for
   next"}`, resident scene retained.
7. **A malformed sidecar keeps the previous live rules**: `reload-config`
   responded `invalid_config` and the following `next` still reported
   `no eligible scene for next`.
8. **No added selection triggers**: exactly one `trigger=startup` selection
   trace and zero `ClockTick`/`WeatherUpdated`/`ConfigChanged`/`OutputChanged`
   selections across the phase while frames and animation frames kept
   presenting (`trace gpu-frame presented`, `trace animation-frame … fps=15`).
9. `worldwallctl … weather` reported `state = disabled` (no provider request
   started by configuration, editing, or applying rules).

## Phase B — startup with no eligible scene fails closed

With both pools disabled, the fresh child exited **1 before presentation** with
the exact bounded diagnostic `startup selection unavailable: no eligible
scene`, and its log contains **no** `trace gpu-frame presented` — the
fail-closed contract (WW-06) is live behavior, not only a unit test.

## Phase C — known-input match and mismatch (explicit smoke-test location)

A second config sets `[location]` to clearly-labelled mockup values (0.0, 0.0,
`Etc/UTC`) — explicit test data, not an inferred or recommended location. Rules:
`smoke-a` with all six time values (always matches a known time), `smoke-b`
with `time_of_day = ["polar_day"]` (never at the test location), `smoke-c` with
`weather_any = ["clear"]` (weather still unknown), and unconstrained
`canvascycle-v25heat`.

- `civil-time configured=true` in the trace (known time).
- Startup selected `canvascycle-v25heat` and `next` selected `smoke-a` — the
  always-matching and unconstrained candidates only. The known-time mismatch
  (`smoke-b`) and the unknown-weather criterion (`smoke-c`) stayed filtered.

## Visual/host evidence

Retained under `~/.local/state/worldwall/s12-implementation-20260923/pool-smoke/`:

- `nested-pool-mode.png` — the nested Hyprland window (its "started without
  start-hyprland" banner identifies the nested instance) rendering the
  pool-selected scene through the real Vulkan path on the assigned GPU;
- `outer-session-during-smoke.png` — the outer session during Phase A,
  showing the production wallpaper session running undisturbed;
- `worldwall-a.log`, `worldwall-b.log`, `worldwall-c.log` — the three phase
  transcripts (selection traces, reload/no-op behavior, fail-closed startup).

Nested instance isolation was asserted in the runner: the nested socket differs
from the outer `wayland-1` before any client starts.

## Failure paths observed live (bonus evidence)

- An earlier configuration where the library root was not resolvable produced
  the bounded error `pool library validation failed: cannot open scene library
  … No such file or directory` and exited before presentation — one-candidate
  validation failing closed, no partial state.
- `reload-config` with `schema_version = 99` sidecar → `invalid_config`, live
  rules retained (assertion 7).

## Findings and follow-ups (not resolved here)

- **[FM-0016](../knowledge/failures/FM-0016-nested-harness-single-output-traces.md)**
  (reproduced): the preserved `tools/run_nested_s01.sh` regression check asserts
  single-output teardown traces that today's `run_multi()` entry point cannot
  emit, and nested `output remove` recreates monitors on this host, so the
  harness cannot pass even with correct behavior. It was deliberately **not**
  modified (weakening a regression check requires an integrator decision);
  repair options and the second composed mechanism are recorded in FM-0016.
- The reusable isolation technique (private `XDG_RUNTIME_DIR` + absolute
  `WAYLAND_DISPLAY` + `worldwallctl --socket` on nested Hyprland) is recorded
  as [BP-0011](../knowledge/best-practices.md).

## Not run / limitations

- No CR-0006 adaptive mode: `adaptive_scenes = true` requires the Open-Meteo
  provider, which would issue a weather request outside this authorization.
  Adaptive pool filtering remains covered by offline tests only.
- No physical suspend/resume, output hotplug, or multi-output pool-mode run
  inside the nested compositor (single nested output).
- No synthetic keyboard/mouse input on the host (unchanged gap from the AC3–AC7
  record: no `wtype`/`ydotool` on the assigned host).
- The production wallpaper was never run in pool mode; its session config is
  untouched, so enabling pool mode on the session remains a separate,
  authorized change.
- Smoke packages are mockup/derived local test material with `unreviewed`
  license state; nothing was committed, published, or relicensed.

## Reproduction

```sh
# From a Wayland session on the assigned host; needs the private derived
# fixtures under fixtures/untracked/effectgames.
WORLDWALL_SMOKE_ARTIFACTS=~/.local/state/worldwall/s12-implementation-20260923/pool-smoke \
  bash tools/run_s12_pool_smoke.sh
```

Recorded runs: three consecutive full passes on 2026-09-23 (Hyprland 0.56.2,
NVIDIA RTX 3070 Ti, Vulkan, debug build of the working tree), exit status 0.
