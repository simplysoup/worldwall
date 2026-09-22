# S02 reviewer guide — safe GPU presentation

This is the step-by-step procedure for reviewing S02 against [S02-AC1 through S02-AC5](../stages/02-gpu-presentation.md). It supplements the [evidence template](../templates/evidence.md); it is not acceptance evidence by itself.

The reviewer must record facts from the assigned host, not infer a pass from a screenshot, a unit test, or a nested/headless compositor. A criterion is `pass` only when its complete assertion is demonstrated. Use `not-run` or `blocked` for the remainder and leave S02 in progress.

## 0. Safety and preparation

1. Review the [delivery contract](../delivery-contract.md), [architecture ownership rules](../architecture.md), [S02 stage](../stages/02-gpu-presentation.md), [ADR-0002](../decisions/ADR-0002-s02-wayland-wgpu-ownership.md), and the current [S02 evidence](../evidence/2026-09-21-s02-gpu-presentation.md).
2. Perform live tests only on the assigned Hyprland/Vulkan host. Do not remove or resize an output in somebody else's session.
3. Before any output-mode test, record the current topology and obtain explicit authorization for the temporary mode change. Restore the original topology in a shell `trap` even if the test fails.
4. Do not run the nested harness if it would connect to or control the outer compositor. A nested test must report a distinct Hyprland instance and Wayland socket before the client is started.
5. Create a review artifact directory outside the repository. Screenshots can contain desktop content; redact or restrict access and record retention details.

```sh
cd /home/soup/Documents/worldwall
review_dir="${XDG_STATE_HOME:-$HOME/.local/state}/worldwall/s02-review-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$review_dir"
hyprctl monitors -j >"$review_dir/monitors-before.json"
hyprctl version >"$review_dir/hyprland-version.txt"
rustc --version >"$review_dir/rust-version.txt"
cargo tree --locked >"$review_dir/cargo-tree.txt"
```

Record the kernel, GPU/driver, Vulkan version, compositor version, output mode/scale/transform, background workload, and exact source revision or working-tree hashes in the evidence record.

## 1. Baseline software checks

Run these from the repository root. Save stdout/stderr and the exit status.

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo build --locked
python3 tools/check_docs.py
python3 -m unittest discover -s tools/tests -v
```

Expected: all commands exit zero; the Rust test suite includes the actual wgpu acquisition variants, zero-extent rejection, capability format selection, and S01 lifecycle tests. These checks establish the baseline but do not pass the live gates.

## 2. Raw-handle and drop-order review — S02-AC2

Review the code before running the compositor test.

```sh
rg -n '\bunsafe\b|transmute|static' src/renderer.rs src/wayland_surface.rs Cargo.toml
```

Confirm all of the following in `src/renderer.rs` and ADR-0002:

- `create_surface_unsafe` is the only unsafe boundary.
- The display pointer comes from the retained SCTK/Wayland connection.
- The window pointer comes from the retained layer `WlSurface`.
- `Renderer<'wayland>` borrows the Wayland surface; there is no transmute, leak, or fabricated `Surface<'static>`.
- GPU calls remain on the event-loop thread.
- The local `LayerSurface` outlives `Renderer`; renderer fields release pipeline/queue/device before surface and instance.
- Acquired `SurfaceTexture` values are local to `render` and cannot survive renderer teardown.
- Output close, layer close, and compositor disconnect stop work before the Wayland owners are dropped.

Reviewer result: record `pass` for the proof only after signing off on ADR-0002. The proof does not by itself pass the live stress portion of S02-AC2.

## 3. Startup and visible-content check — S02-AC1, AC3, AC4

Run Worldwall in the assigned session from a terminal that is not the ordinary app being used for the input check.

```sh
set -o pipefail
log="$review_dir/startup.log"
timeout --signal=TERM 20s cargo run --locked 2>&1 | tee "$log"
status=${PIPESTATUS[0]}
printf 'worldwall exit status: %s\n' "$status" | tee "$review_dir/startup-status.txt"
```

The timeout status `124` is expected for this bounded idle run. It is not a teardown pass.

Check the trace order:

```sh
awk '/trace (globals-ready|role-settings|initial-commit|configure |gpu-adapter|gpu-surface configured|frame-requested|gpu-frame presented|frame-callback)/' "$log"
! grep -E 'gpu-uncaptured-error|gpu-device-lost' "$log"
! grep -E 'physical=Some\(PhysicalExtent \{ width: 0|height: 0' "$log"
```

Expected order is:

1. `initial-commit buffer=none awaiting-configure=true`
2. `configure ... ack=toolkit-before-handler` with a nonzero physical extent
3. Vulkan adapter selection and capabilities-derived FIFO configuration
4. `frame-requested before-present=true`
5. `gpu-frame presented`
6. `frame-callback ready=true`

The `frame-requested` trace is the client-side ordering proof. Review `Renderer::render` and the pinned wgpu API to confirm that `Queue::present` performs the presentation commit and that Worldwall does not issue a duplicate manual commit.

### Visible ordinary-app procedure

1. Identify the selected output from `globals-ready selected-output=...`.
2. Put an ordinary Wayland application, such as Alacritty, on that output and focus it.
3. Record focus before Worldwall starts:

   ```sh
   hyprctl activewindow -j >"$review_dir/active-before.json"
   ```

4. Start Worldwall and wait for `gpu-frame presented`.
5. Record focus again:

   ```sh
   hyprctl activewindow -j >"$review_dir/active-after.json"
   ```

6. Verify the application remains above the Background layer and remains the active window. Interact with it manually: move the pointer into the application, click, and type a harmless line of text. Record what happened, not just a screenshot.
7. Optionally capture appearance with `grim` after the interaction. A screenshot supports appearance only; it does not prove input, ownership, ordering, or teardown.

The full AC1 pass requires both visible content and manual input pass-through/no focus capture. If pointer interaction was not performed, record AC1 as `not-run` even if the screenshot is correct.

## 4. Idle and callback check — S02-AC4

After the bootstrap frame, leave Worldwall idle for at least 10 seconds while ordinary applications remain open.

```sh
before=$(grep -c 'trace gpu-frame presented' "$review_dir/startup.log" || true)
sleep 10
after=$(grep -c 'trace gpu-frame presented' "$review_dir/startup.log" || true)
printf 'presented-before=%s presented-after=%s\n' "$before" "$after"
```

For the static S02 scene, there should be no continuous frame stream. Review the log for:

- no frame request after the initial frame unless a configure occurs;
- request-before-present ordering;
- no repeated retry/error storm;
- no busy loop while idle.

A process-level CPU sample may support the observation, but it is not the S04/S11 performance measurement.

## 5. Real resize/reconfigure check — S02-AC3 and AC4

This requires a live output configure on the assigned host. Do not guess a compositor command or change persistent configuration. First save the original monitor JSON, get authorization, and use the host's documented temporary output-mode mechanism to switch the selected output to another supported nonzero mode (for example, a supported 1280×720 mode). Record the exact command used.

Run Worldwall with a log as in step 3, then:

1. Wait for the initial `gpu-frame presented`.
2. Apply the temporary output mode change.
3. Wait for a new `configure` trace and a `gpu-surface reconfigured extent=...` trace.
4. Verify every configured width and height is nonzero and the new dimensions match the compositor's physical extent.
5. Verify a frame callback is requested before the resized presentation.
6. Restore the exact original mode and record the second configure/reconfigure sequence.
7. Stop Worldwall only after the restore is complete.

Useful checks:

```sh
grep -E 'trace (configure |surface-scale|gpu-surface reconfigured|frame-requested|gpu-frame presented)' "$review_dir/resize.log"
! grep -E 'extent=0x|physical=.*width: 0|physical=.*height: 0' "$review_dir/resize.log"
hyprctl monitors -j >"$review_dir/monitors-after.json"
```

Expected: no zero-size `wgpu::Surface::configure`, no stale-size presentation, no panic, and no scene reselection. If the output command changes the session but produces no configure, record `blocked`/`not-run`; do not infer resize behavior from changing an ordinary window size.

## 6. Surface failure and bounded-retry review — S02-AC5

The pure test covers the pinned wgpu variants directly:

```sh
cargo test --locked renderer::tests::actual_surface_error_variants_have_bounded_outcomes -- --exact
```

The debug build also provides a one-shot, explicit review seam. It is absent from release builds and does not alter the default acquisition path:

```sh
for error in timeout occluded outdated lost validation; do
  WORLDWALL_TEST_SURFACE_ERROR="$error" \
    timeout --signal=TERM --kill-after=3s 8s cargo run --locked
 done
```

Review that `Timeout` and `Occluded` skip one frame, `Outdated` and `Lost` take exactly one bounded recovery path and then present, and `Validation` is fatal. Confirm each injected value is consumed once and the recovery path cannot loop indefinitely. Record the process status and trace for every value.

The seam is a reviewed debug-only test path, not a claim that a physical GPU fault was induced. Never provoke a device failure by killing the display server, unloading a driver, or damaging the user's session. A real injected surface-failure behavior remains additional evidence when available, not a reason to weaken the bounded policy.

Record any `gpu-uncaptured-error` or `gpu-device-lost` line as a failure requiring diagnosis, not as a pass because the process eventually exited.

## 7. Close, output removal, and compositor disconnect — S02-AC2 and AC5

Prefer the nested real-compositor harness because it prevents damage to the outer desktop:

```sh
bash tools/run_nested_s01.sh
```

Before accepting its result, verify that the harness discovered a child Hyprland PID and a Wayland socket distinct from the outer instance. Inspect the retained logs if it fails. Expected client traces include:

- GPU presentation before the lifecycle event;
- `layer-closed` and/or `selected-output-removed` for output removal;
- `trace compositor-disconnect` for compositor shutdown;
- `owners-dropped gpu-before-layer-before-selected-output=true`;
- no invalid-handle, validation-error, panic, or retry storm.

The harness is not a pass if it accidentally connects the client to the outer compositor, if its socket lock conflicts, or if it only proves a mock/headless path. Do not work around a nested socket conflict by removing an outer physical output.

For an assigned-host manual run, use a dedicated nested compositor or an explicitly authorized disposable session. Start Worldwall, wait for the first presentation, then stop only that compositor. Save both compositor and client logs. Confirm the client exits through its disconnect path and drops GPU resources before Wayland owners.

## 8. Complete the evidence record

Use a new append-only review record or append a clearly dated review section to the S02 evidence. For every AC row include:

- exact command or manual steps;
- operator and reviewer identity;
- host/compositor/GPU/Vulkan/dependency versions;
- source revision or working-tree hashes;
- expected and observed facts;
- one outcome: `pass`, `fail`, `blocked`, or `not-run`;
- artifact path and checksum, with retention/access instructions;
- limitations and any linked roadblock.

Do not turn partial results into a stage pass. In particular, a screenshot is not a substitute for input, resize, callback, lifetime, or teardown evidence.

## 9. Reviewer decision and ledger update

When every S02 criterion has complete evidence and ADR-0002 is approved:

1. The assigned reviewer records the decision and date in the evidence record.
2. The integrator changes S02 in `docs/roadmap.md` from `in-progress` to `review` only when the complete evidence is submitted.
3. The maintainer/acceptance authority, not the implementer, changes `review` to `accepted` and links the evidence in the roadmap.
4. Update `docs/status.md` with the actual result and remaining limitations.
5. If any criterion is blocked or failed, leave S02 in progress or blocked and record the next safe experiment; do not start S03 as if S02 were accepted.

## Current known S02 gaps

At the time this guide was written, the repository evidence has a Vulkan bootstrap/presentation trace, a visual/focus observation, unit tests, and an S01 diagnostic path. It does **not** yet contain complete assigned-host evidence for pointer interaction, real output resize, injected surface failures, or compositor disconnect after GPU presentation. Those gaps are intentional and must be resolved or explicitly recorded before acceptance.
