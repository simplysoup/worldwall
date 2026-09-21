# Validation and evidence policy

## Checks available now

From the project root, Python 3.10+:

```sh
python3 tools/check_docs.py
python3 -m unittest discover -s tools/tests -v
```

The dependency-free checker verifies local Markdown destinations and heading anchors (simple GitHub-style headings), balanced fences, required documents/stage sections, stage coverage/dependency references/states, skill frontmatter/names, and the original archive hash. It ignores remote-link reachability and does not certify technical correctness or approval. Tests exercise checker failure cases. Supported Markdown/YAML syntax and parser limits are documented in [tooling conventions](../tools/README.md). No browser, network, or Rust toolchain is required for these checks.

## Checks to establish in S00

Only after `Cargo.toml` and targets exist, verify and document these commands against the pinned toolchain:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --locked
```

If future feature combinations are mutually exclusive, approve/document an explicit tested matrix instead of silently skipping features. Mock/headless checks must not be labelled compositor acceptance. Add CI once the scaffold exists; documentation-only setup must not imply CI is already installed.

## Test layers

| Layer | Suitable checks | Cannot prove |
| --- | --- | --- |
| Pure unit | Selection, scaling math, schema validation, fake clock/cache, scheduler state | Wayland protocol ordering, actual GPU color, power behavior |
| Integration / GPU headless where available | Decode/upload/readback, shader vectors, cache transactions, IPC framing | Visible layer behavior or real suspend/hotplug |
| Live Hyprland/Vulkan | Configure/ack/present, input pass-through, resize, scale, output removal, suspend | General compatibility with untested hardware |
| Fault injection | Invalid assets, late weather, timeouts, surface failures, missing services | Actual recovery unless assertions and final state are recorded |
| Repeated resource benchmark | CPU/GPU/memory, frame pacing, network, idle/no-output behavior | Universal power savings from one machine |

## Acceptance record

Use the [evidence template](templates/evidence.md) and retain it in `docs/evidence/`. Every `Sxx-ACn` needs:

- Revision or immutable artifact identity; if no Git commit exists, record path and hash.
- Date, operator, reviewer, exact command/manual procedure, and environment versions.
- Expected/observed result and outcome: `pass`, `fail`, `blocked`, or `not-run`.
- Log/readback/capture/metrics location, with sensitive information redacted.
- Limitations and linked roadblocks, not a vague “works on my machine”.

Hardware records include compositor, kernel, GPU, driver, Vulkan, dependency versions, output mode/scale/transform, scene, FPS target, and background workload. A screenshot supports appearance but does not prove input behavior, ordering, no CPU recoloring, or safe teardown. Record those separately. Large artifacts may live outside Git with stable location, checksum, and retention/access instructions; never depend only on an ephemeral chat attachment.

## Resource measurement procedure (S04 baseline, S11 acceptance)

S04 measures the implemented single-output static/animated paths. Mark later-stage workloads `not-run` with their owning stage; do not build multi-output or suspend early just to populate this baseline. S11 requires the full approved matrix and numerical limits. Earlier measurements are observational, not a substitute for release-budget approval.

1. Before collecting release results, the maintainer approves numeric limits for CPU (state percent-of-one-core convention), RSS/VRAM, frame pacing, GPU utilization/power where measurable, and network cadence on named hardware. Missing limits block S11; they are not zero or automatically satisfied.
2. Record an idle-compositor baseline under the same scene/output/environment conditions, then static and animated Worldwall at 15 and 30 FPS. Include one and multiple outputs, and no-output/suspended states where supported.
3. Use a 60-second warmup followed by three 5-minute samples per workload. Record sampling tools, intervals, raw data, summary statistics, and background process controls. Keep startup/scene-upload costs separate from steady state.
4. Use per-output submission timestamps/counters to verify no steady-state intervals shorter than the configured frame interval beyond documented clock precision. Reconfigure/bootstrap frames are tagged separately, not hidden in the average.
5. Track full texture uploads: zero steady-state scene uploads, apart from documented content changes or recovery. Weather timers must not increase selection-generation counts.
6. Compare against approved limits and the previous accepted baseline; explain regressions. Unsupported power counters are `unavailable`, not measured zero. If a required metric is unavailable, agree on an approved alternative or block acceptance.

## Required failure coverage

Stages own their immediate failure paths. By S11 include: configure with incomplete dimensions, surface lost/outdated/timeout, device failure, invalid/oversized asset, stale async scene completion, corrupt cache, offline startup, slow weather, duplicate resume signals, missing D-Bus integration, mixed-scale outputs, unplug during rendering, zero outputs, malformed/unauthorized IPC, compositor disconnect, and repeated shutdown/startup. See [failure modes](knowledge/failure-modes.md).
