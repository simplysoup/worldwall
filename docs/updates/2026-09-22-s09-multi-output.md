# S09 implementation increment — shared multi-output lifecycle and live topology tests

- Date / operator: 2026-09-22 · Pi coding agent
- Revision or artifact identity: working tree based on `c8ba3fc14b633d31bc31c780f1d84ab4f9a2f3e8`; live bundle `$HOME/.local/state/worldwall/s09-live-20260922/` with `SHA256SUMS`
- Stage / requirements: S09; WW-01, WW-02, WW-05, WW-09, WW-11; implementation and test increment for S09-AC1 through S09-AC5
- Environment: assigned `soupydesktop` session, Hyprland 0.56.2, Vulkan 1.4.357, NVIDIA RTX 3070 Ti, two HDMI outputs; Rust pinned by `rust-toolchain.toml`
- Scene/config: synthetic reference scene, default 30 FPS, weather disabled, no location configuration; output mode changes were restored after each test
- Review state: S09 accepted by `simplysoup` on 2026-09-22; CR-0004 records the approved future-work deferral for physical incompatible-output evidence

## Implemented slice

- Added `SharedRenderer` and output-local `OutputRenderer` in `src/renderer.rs`. Device, queue, scene textures, scene metadata, bind-group layout, and cached format pipelines are shared. Each output owns its surface/configuration, cover-mapping uniform, animation-clock uniform, bind group, callback-facing render path, and bounded surface recovery.
- Reworked `src/wayland_surface.rs` around keyed output records. Layer owners remain outside GPU presenters; topology rebuilds drop all presenters before removing or recreating layer owners. Output events do not invoke scene selection.
- Added independent output callbacks/schedulers, physical extent tracking, resize/scale/transform handling, zero-output timer cancellation, reconnect recovery, and capability validation. Incompatible outputs enter a bounded degraded set and are retried only after rediscovery, while compatible outputs continue.
- Fixed the reconnect race found during live testing: an output that becomes configured after the first topology pass now triggers one presenter rebuild. The bounded degraded set prevents that readiness check from retry-spinning on an incompatible output.
- Added unit coverage for actionable incompatible-capability failures and retained the output-uniform independence tests.
- Added cumulative explicit wgpu resource-creation counters for scene textures/buffers/layouts, upload bind groups, pipelines, output uniform buffers, output bind groups, and output surface configurations. These are application resource-event counters, not driver allocator or VRAM measurements.
- Added a debug-only `WORLDWALL_TEST_WITHHOLD_OUTPUT=<protocol-id>` seam to withhold one output's frame callback while other output schedulers continue. This is a bounded stress-test aid, not production behavior or a substitute for a physically slow/hidden output.
- Added optional `wp_fractional_scale_v1` and `wp_viewporter` integration for each layer surface. Fractional outputs keep `wl_surface` buffer scale 1, wait for the compositor's preferred 120-based scale, allocate the rounded-up physical extent, and set viewport source to the physical buffer and destination to the logical layer size. Integer scaling remains the fallback when either protocol is absent.

## Follow-up implementation validation

| Slice | Exact procedure / command | Observed result | Outcome | Artifact |
| --- | --- | --- | --- | --- |
| Allocation counters | `cargo fmt -- --check`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo test --locked --all-targets --all-features`; `cargo build --locked`; `WORLDWALL_TEST_EXIT_AFTER_MS=9000 target/debug/worldwall` | 73 non-ignored unit tests passed; build passed; assigned session reported one shared scene upload, 3 scene textures, 5 scene buffers, 1 layout, 1 upload bind group, 2 pipelines, 4 output uniform buffers, 2 output bind groups, and 2 output initial-surface configurations for two presenters; clean deadline exit | pass for instrumentation and ordinary assigned-host trace; not allocator/VRAM evidence | `/tmp/worldwall-s09-allocation-live.log` (SHA-256 `f2b02e089890564d3013c5191cc7271ef67015f0e6cd35ae49fd417e859e23bf`) |
| One-output callback withholding | `cargo build --locked`; `WORLDWALL_TEST_WITHHOLD_OUTPUT=106 WORLDWALL_TEST_EXIT_AFTER_MS=5000 target/debug/worldwall` on the assigned session | Output `106` received one `withheld=true` callback and submitted zero steady-state frames; output `107` continued for 138 frames; no fatal, panic, or error trace; clean deadline exit | implementation/fault-seam evidence only; slow/hidden-output acceptance gate remains open | `/tmp/worldwall-s09-withheld-output-live.log` (SHA-256 `bf0de830cf380063272632d9b6aa177761983371e1ef67445a6c63c7a847b4c6`) |
| Fractional-scale protocol path | `cargo fmt`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo test --locked --all-targets --all-features`; `cargo build --locked`; authorized `hyprctl eval 'hl.monitor({ output = "HDMI-A-1", scale = 1.25 })'`; `WORLDWALL_TEST_EXIT_AFTER_MS=9000 target/debug/worldwall`; explicit restore rule | 75 non-ignored unit tests passed; assigned compositor advertised both protocols; HDMI-A-1 emitted preferred scale `150`, configured viewport source `1920x1080` to logical destination `1536x864`, and presented alongside HDMI-A-2 at preferred scale `120`; no fatal/panic/error trace; clean deadline exit; both outputs restored to scale 1 with original modes/positions | pass for non-integer preferred-scale protocol/state/viewport live path on the assigned host; S09 acceptance is recorded in the acceptance review below | `/tmp/worldwall-s09-fractional-noninteger-live.log` (SHA-256 `bb722835f68876e4223ec46cdfce1c5da7f094f15925dceb3e2b05d587ab6bfd`) |

## Validation results

| Criterion | Exact procedure / command | Observed result | Outcome | Artifact |
| --- | --- | --- | --- | --- |
| Implementation checks | `cargo fmt -- --check`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo test --locked --all-targets --all-features`; `cargo build --locked` | Formatting, clippy, build passed; 75 tests ran, 72 passed and 3 pre-existing Vulkan readback tests remained ignored | pass for implementation checks | command output in session |
| S09-AC1 mixed size/refresh | Hyprland Lua monitor evaluation configured HDMI-A-1 `1920x1080@144` and HDMI-A-2 `2560x1440@59.95`; `WORLDWALL_TEST_EXIT_AFTER_MS=9000 target/debug/worldwall`; restore rules after run | Configure traces reported `1920x1080` and `2560x1440`; both presenters used `Rgba8UnormSrgb`, both emitted independent `fps=30` frames, and shared scene uploads remained `1` | pass for tested integer-scale/normal-transform matrix | `mixed-initial.log` |
| S09-AC1 live resize | Running client was changed to HDMI-A-2 `2560x1440@59.95`, then restored to `1920x1080@60` | Output configure/reconfigure traces changed only the affected output extent and reported `scene-upload=false`; no selection trace was emitted | pass for tested resize path | `mixed-resize.log` |
| S09-AC1 transform and integer scale | Running client was changed to transform `1` (`_90`), then scale `2`, then restored | `_90` configured as `1080x1920`; scale `2` configured as `960x540` logical and `1920x1080` physical; both changes reconfigured the output without scene upload | pass for tested normal/90-degree and integer-scale paths | `transform-scale.log` |
| S09-AC1 fractional scale (initial increment) | Hyprland monitor scale `1.25` was attempted and then restored | The original client exposed integer Wayland scale `2` with logical `1536x864`; protocol support was not yet implemented in that artifact | historical not-run / superseded by the follow-up implementation slice | `fractional-scale-unsupported.log` |
| S09-AC2 shared resources | Mixed-size live run and source inspection | One `gpu-shared-scene` upload, one shared Vulkan adapter, two output presenters, output-local uniform update counters; no steady-state scene re-upload | pass for observed shared-resource invariant; detailed application allocation counters were added in the follow-up slice below; driver allocator/VRAM counters are not instrumented | `mixed-initial.log` |
| S09-AC3 hotplug/unplug, zero output, reconnect | While the client ran, disabled HDMI-A-2, disabled HDMI-A-1, re-enabled HDMI-A-1, then re-enabled HDMI-A-2 | Output removal dropped presenters; `outputs-zero frame-wakeup=false dispatch-idle=true` appeared; reconnect restored rendering. The post-fix run added both presenters after the second output's delayed configure | pass for tested topology path | `hotplug-zero-reconnect.log` |
| S09-AC3 topology during sleep | Authorized custom `dbus-monitor` helper; on `PrepareForSleep=true`, retried disabling HDMI-A-1 until observed disabled; on resume restored it; `systemctl suspend --no-wall` | Helper observed `disabled=true` then `disabled=false`; Worldwall recorded pause, one coalesced resume, retained shared scene, and resumed presentation. One shared scene upload and no selection-generation trace occurred | pass for this assigned-host subcase | `topology-sleep.log`, `topology-sleep-helper.log` |
| S09-AC4 pacing | Same mixed-size run; both outputs at different compositor refresh rates | Both outputs emitted independent `fps=30` traces with separate uniform counters; no panic or fatal/error trace | pass for tested ordinary mixed-refresh path; slow/hidden-output stress remains open | `mixed-initial.log` |
| S09-AC5 selection and incompatibility | Source path review; `renderer::incompatible_output_capabilities_are_bounded_and_actionable` unit test | Capability failures for no sRGB, no FIFO, and no alpha are actionable; degraded output state is bounded and does not create a second device. No physical incompatible adapter/output was available on this host | unit/source pass; physical incompatible-output gate not-run | test output and source paths |

## Acceptance review

| Criterion | Acceptance result | Evidence / limitation |
| --- | --- | --- |
| S09-AC1 | accepted | Mixed-size/refresh, resize, transform, integer-scale, and assigned-host non-integer fractional-scale/viewport traces passed. No claim is made for arbitrary transforms or untested compositors. |
| S09-AC2 | accepted | Shared adapter/scene traces and application allocation counters show one shared scene resource set with output-local buffers/bind groups. Driver allocator/VRAM statistics remain unavailable. |
| S09-AC3 | accepted | Hotplug, zero-output, reconnect, resize, and topology-during-sleep traces passed with owner-safe teardown and no fatal trace. |
| S09-AC4 | accepted | Ordinary mixed-refresh live pacing and the bounded one-output callback-withholding stress seam passed. A physically slow/hidden-output fixture was unavailable; no universal pacing claim is made. |
| S09-AC5 | accepted with CR-0004 limitation | Selection generation remained unchanged and the unit/source capability-failure path is bounded/actionable while compatible outputs remain usable. Physical incompatible-capability output evidence is explicitly `not-run` and future work under [CR-0004](../changes/CR-0004-s09-incompatible-output-evidence-deferral.md). |

`simplysoup` accepted S09 on 2026-09-22 after reviewing the implementation, unit/source checks, assigned-host multi-output/topology traces, fractional-scale live trace, and the CR-0004 evidence deferral. This acceptance does not claim a physical incompatible-output result, universal adapter compatibility, driver allocator/VRAM measurements, or a physical slow/hidden-output stress result.

## Failed/recovered test attempt

The existing `tools/run_s08_topology.sh --authorized` helper was also run against the multi-output binary. It failed its assertion because the immediate state query after the sleep signal observed `disabled=false`; it did not prove the required transition. The test was not counted as a pass. The follow-up helper used bounded retries and did prove both states; the failed logs remain at `/tmp/worldwall-s09-topology-sleep-live.log` and `/tmp/worldwall-s09-topology-sleep-helper.log` for diagnosis.

## Incompatible-output evidence plan

1. The maintainer should authorize a named physical fixture: keep one known-compatible output active and attach an output whose actual Wayland/Vulkan surface capabilities are incompatible with the selected shared adapter (for example, no required sRGB format, no FIFO mode, or no usable alpha mode). Do not manufacture this by changing production capability checks or by using only a mocked `SurfaceCapabilities` value.
2. Record the physical output identity/mode, compositor and driver versions, Vulkan adapter identity, and the exact capability query result before starting Worldwall. Preserve the normal compatible output as the control.
3. Run the multi-output binary with a bounded exit deadline. Expected evidence is one actionable `output-degraded` record for the incompatible output, no second adapter/device, no retry storm, no selection-generation advance, continued frames on the compatible output, and clean teardown.
4. Remove and re-add the physical output, or rediscover it after the compositor reports the change, and verify the bounded degraded state is retried only on rediscovery. Restore the original monitor topology with an explicit operator-authorized trap.
5. If no such physical fixture can be assigned, retain the unit test and source evidence as bounded failure-path coverage, mark the physical gate `not-run`, and record the host/hardware limitation rather than counting a nested/mock run as acceptance.

## Limitations and open gates

- The supported live matrix is currently: one Vulkan adapter/device/queue; integer Wayland output scales plus assigned-host preferred scales `120` and `150` through the fractional-scale/viewport path; normal and tested 90-degree transform; nearest-neighbor cover mapping; sRGB FIFO surfaces. No acceptance claim is made for arbitrary transforms, untested adapters, or incompatible physical outputs.
- Application allocation-event counters are now instrumented and have one assigned-host trace. They count explicit wgpu creation sites, not backend allocator or VRAM usage. A slow/hidden-output stress case remains open, so S09-AC4 retains its evidence limitation.
- No physical incompatible-capability output was available. The pure capability test validates the bounded failure path but cannot replace a live hardware/compositor capability test.
- The live logs are retained outside Git at the artifact path above. `final-monitor-state.json` and the restore procedures show the assigned session returned to two enabled `1920x1080` scale-1 normal-transform outputs.

## Next owner/action

Future work owner: an authorized maintainer-assigned live-test operator may run the slow/hidden-output and physical incompatible-output fixtures when available. The physical incompatible-output result remains explicitly `not-run` under CR-0004; any future result must preserve the artifact and topology-restoration requirements in the evidence plan. S09 is accepted with these documented limitations.
