# FM-0016 — Nested regression harness asserts single-output teardown traces the entry point cannot emit

**Classification:** resolved-with-regression-test (2026-09-23; previously
reproduced) · **Areas:** tools, S01/S02 regression checks, S09 ·
**Related:** [S12 pool smoke](../../updates/2026-09-23-s12-ac4-ac5-live-pool-smoke.md),
[tooling](../../../tools/README.md)

## Trigger

Running the preserved regression check `tools/run_nested_s01.sh` on the
assigned Hyprland host while a production Worldwall instance is active (and
also with its client running under today's entry point).

## Symptom

The harness starts a nested Hyprland and a worldwall child that reaches
`trace configure … state=Ready` and `trace gpu-frame presented`, then fails its
`grep` assertions and `wait_client_exit` and reports `nested S01 harness
failed; logs retained …`. The child log shows
`trace layer-closed output=NN state=closing` and
`trace output-removed id=NN state=removed` instead of the asserted
`trace layer-closed state=closing owners-dropped=true` /
`trace selected-output-removed` lines.

## Suspected mechanism

Two independent mechanisms compose:

1. The asserted teardown traces (`trace owners-dropped
   gpu-before-layer-before-selected-output=true`, `trace selected-output-removed
   id=… owners-dropped=true`, `trace layer-closed state=closing
   owners-dropped=true`) are emitted only by the single-output `WaylandClient`
   path (`wayland_surface.rs` around the `shutdown_owners` teardown). Since the
   S09 refactor, `main()` runs `run_multi()` (the `MultiWaylandClient` path),
   which emits different traces (`trace layer-closed output=… state=closing`)
   and deliberately supports zero-output/reconnect instead of exiting when an
   output disappears.
2. On this host's nested Hyprland (0.56.2), `output remove` does not leave a
   stable zero-output state: the nested compositor recreates virtual monitors,
   so the multi-output client correctly re-adds layers for each new output and
   never reaches the harness's expected exit condition.

## Confirmed cause

Confirmed for mechanism 1 by source inspection: the asserted strings exist only
in the single-output teardown that `main()` cannot reach. Mechanism 2 is
observed in the retained logs (repeated `outputs-zero → output-added` cycles
with new `protocol-id`s) but the compositor-side reason is not isolated.

## Impact

The S01/S02 nested regression check cannot pass against the current binary
entry point even when the renderer and lifecycle behave correctly, so its
"locked-check output" no longer guards S01/S02 behavior. Live lifecycle
coverage for the multi-output path currently comes from the S09/S10 assigned
sessions and `tools/run_s12_pool_smoke.sh`.

## Prevention / recovery

Do not "fix" the harness by weakening or deleting its assertions without an
integrator decision: the correct repair is either a test seam that runs the
retained single-output `run()` path (the documented diagnostic baseline) or an
approved update of the assertions to the S09 teardown contract. A second,
independent live check (`tools/run_s12_pool_smoke.sh`) exists for pool-mode
runtime behavior and distinct-socket isolation.

## Resolution (2026-09-23)

The maintainer delegated the disposition to the implementer, who took the
first repair option so the preserved assertions keep guarding their original
S01/S02 contract:

- `src/main.rs` selects the retained single-output `run()` baseline only when
  the explicit diagnostic seam `WORLDWALL_SINGLE_OUTPUT_BASELINE` is set;
  normal operation is unchanged (`run_multi()`).
- `tools/run_nested_s01.sh` sets that seam for its clients and documents why.
  Its assertions are unmodified.
- Recorded passing run: `nested Hyprland S01 lifecycle + S02 presentation
  checks passed (renderer=gles2)` — both the compositor-driven output-removal
  teardown (`trace layer-closed state=closing owners-dropped=true`,
  `trace selected-output-removed …`, `trace owners-dropped
  gpu-before-layer-before-selected-output=true`) and the real disconnect
  (`trace compositor-disconnect …`) against a nested Hyprland on the assigned
  host.

Mechanism 2 proved moot for the baseline: the single-output path shuts down on
its selected-output removal before any recreated nested monitor matters (the
monitor flap only affects the multi-output path's legitimate reconnect
behavior). The production entry point (`run_multi()`) keeps its S09
hotplug/reconnect contract, which remains covered by the assigned S09/S10
sessions and `tools/run_s12_pool_smoke.sh`.

## Regression test

`tools/run_nested_s01.sh` is itself the regression check and is green again
(both phases, recorded 2026-09-23). The `WORLDWALL_SINGLE_OUTPUT_BASELINE`
seam is a documented test/diagnostic aid in the style of `WORLDWALL_LAYER_ONLY`;
it is not a release behavior.

## Revalidation trigger

Any change to teardown traces, the `main()` entry point or the baseline seam,
a nested Hyprland upgrade, or recurrence of a harness failure.
