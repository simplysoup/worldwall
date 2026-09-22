# Worldwall

A planned GPU-accelerated, weather- and time-aware pixel-art wallpaper engine for Hyprland/Wayland.

**Current state:** S00 foundation, S01 single-output Wayland lifecycle, S02 safe GPU presentation, S03 indexed-scene/M1, and S06 civil-time/astronomy policy are accepted. S04 palette animation and pacing and S05 registry selection are also accepted. S07 Open-Meteo weather support is implemented under accepted ADR-0006, with runtime use explicitly opt-in and one bounded live trace recorded.

## Start here

1. [Delivery contract](docs/delivery-contract.md) — scope, invariants, acceptance, and approval.
2. [Roadmap](docs/roadmap.md) — canonical stage order and state.
3. [Current status](docs/status.md) — evidence, next action, and pending decisions.
4. [Documentation map](docs/README.md) — workflows, templates, and reference material.
5. [S02 reviewer guide](docs/reviews/s02-reviewer-guide.md) — step-by-step acceptance walkthrough.
6. [S03 evidence](docs/evidence/2026-09-21-s03-indexed-scene.md) — accepted M1 decoder/resource/live evidence.
7. [Contributor instructions](AGENTS.md) — how to work safely and leave a useful handoff.

**Current action:** S08 suspend/resume orchestration is accepted. The system-bus adapter subscribes to systemd-logind without blocking Wayland, coalesces duplicate signals, pauses animation/deadline/weather timers, and performs one bounded resume refresh/selection when output configuration is valid. Three operator-authorized suspend/resume cycles, offline-weather behavior, failed-candidate retention, and the corrected topology-change-during-sleep subcase are recorded. `simplysoup` approved ADR-0007 and S08 on 2026-09-22; the next planned stage is S09 multi-output and hotplug. See the [S08 implementation and acceptance update](docs/updates/2026-09-22-s08-suspend-resume.md). Open-Meteo remains disabled unless explicitly configured with a provider name and latitude/longitude; weather completions never reselect a scene.

## Build and run the S03 indexed-scene path

S00 pins Rust 1.94.0 and exact dependency versions in `Cargo.toml`/`Cargo.lock`.
The default binary connects to the current Wayland session, binds SCTK output
and layer-shell globals, waits for configure, creates a Vulkan wgpu surface on
the retained Background layer, and presents one static indexed synthetic scene. Set `WORLDWALL_SOLID_COLOR=1` to use the S02 solid-color diagnostic path.
Set `WORLDWALL_LAYER_ONLY=1` to retain the S01 layer-only diagnostic path. Set `WORLDWALL_REGISTRY_DIR=/path/to/registry-root` to load and validate the explicit S05 registry, perform deterministic startup selection from an unknown snapshot (using the designated fallback when no environmental tag matches), and upload that candidate. The older `WORLDWALL_SCENE_DIR=/path/to/package` path remains available for one fixed package; add an optional strict S04 `animation.toml` beside it to enable GPU palette effects. With neither path, the original synthetic reference scene is used. If both paths are set, the validated registry is selected. Set `WORLDWALL_FPS=15` or `WORLDWALL_FPS=30` (the default) to select the pacing target. An optional package `time_of_day.toml` plus contained `time_of_day.png` 256×N atlas enables civil-time palette fades; set the explicit `WORLDWALL_UTC_OFFSET_MINUTES` to supply the configured civil offset. To enable resume-time astronomy selection, also set the explicit `WORLDWALL_LOCATION_LATITUDE` and `WORLDWALL_LOCATION_LONGITUDE`; without the offset/location pair, the environment receives visible unknown-safe time/season values and no host timezone is inferred. Suspend/resume listens to systemd-logind on the system bus when available and reports degraded support when it is missing. Clock updates only the resident palette uniform and never reselect a scene. To explicitly enable Open-Meteo, set `WORLDWALL_WEATHER=open-meteo`, `WORLDWALL_WEATHER_LATITUDE=<decimal>`, and `WORLDWALL_WEATHER_LONGITUDE=<decimal>`. With `WORLDWALL_WEATHER` unset, weather makes zero requests. Coordinates are sent to Open-Meteo over HTTPS and are not logged or cached.

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo build --locked
cargo run --locked
```

The accepted animation schema and GPU layout are recorded in
[ADR-0004](docs/decisions/ADR-0004-s04-animation.md); the additive civil-time
atlas contract is recorded in [CR-0002](docs/changes/CR-0002-s06-time-of-day-palettes.md).
Both files are optional. Unknown fields, overlapping groups, non-finite values,
out-of-range indices, malformed keyframes, and escaping timeline assets are
rejected.

Example `animation.toml`:

```toml
schema_version = 1

[[group]]
name = "water"
effect = "hue_cycle"
range = [1, 3]
speed = 0.25
amplitude = 0.5
phase = 0.0

[[group]]
name = "flags"
effect = "palette_cycle"
indices = [9, 4, 11]
speed = 0.1
amplitude = 1.0
phase = 0.0

[[group]]
name = "reference-water"
effect = "blend_shift"
range = [32, 47]
speed = 0.25
amplitude = 1.0
phase = 0.0
```


Run `cargo run --locked` only in the assigned Hyprland/Wayland session; it stays
alive until the layer closes, its selected output is removed, or the compositor
disconnects. For repeatable lifecycle testing without changing the outer desktop
session, run the nested real-Hyprland harness:

```sh
tools/run_nested_s01.sh
```

The harness launches a separate Hyprland instance using the nested Wayland
backend, runs the client on its separate socket, removes the selected output,
and repeats with a compositor disconnect. It is supplemental lifecycle/GPU
trace evidence, not a replacement for assigned-host acceptance. See the [S02
reviewer guide](docs/reviews/s02-reviewer-guide.md), [S02 evidence record](docs/evidence/2026-09-21-s02-gpu-presentation.md), and [S01 evidence record](docs/evidence/2026-09-21-s01-wayland-lifecycle.md)
for known limitations and exact results.

## Check the documentation

From the repository root, using Python 3.10 or newer:

```sh
python3 tools/check_docs.py
python3 -m unittest discover -s tools/tests -v
```

These commands validate documentation structure, not Worldwall's runtime. See [tooling conventions and limits](tools/README.md). Use the [S02 reviewer guide](docs/reviews/s02-reviewer-guide.md) for renderer and live compositor checks.

## Local developer skills

Project skills live in [`.agents/skills/`](.agents/skills/README.md). They support staged execution, evidence-based updates, roadblock diagnosis, roadmap changes, learning capture, delegation, and renderer review. Pi discovers these skills in a trusted project; other agents can read their `SKILL.md` files directly. No external packages or global configuration changes are required.

## History and licensing

The [original outline](docs/archive/project-outline.original.md) is preserved verbatim; it is historical, not a second specification. See the [audit](docs/audit.md) for corrections and proposed decisions. Project code and documentation are released under the [MIT License](LICENSE). Art assets are intentionally not bundled in this repository; do not redistribute unlicensed assets or infer permission from the Effect Games inspiration link.
