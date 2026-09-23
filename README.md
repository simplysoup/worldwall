# Worldwall

A planned GPU-accelerated, weather- and time-aware pixel-art wallpaper engine for Hyprland/Wayland.

**Current state:** Historical S00–S11 acceptances remain recorded. CR-0005 remains implemented with its separately classified evidence and limitations. CR-0006's approved policy remains in contract v0.6; CR-0007 adds the global half-speed animation multiplier in contract v0.7. Offline code tests do not constitute acceptance or live evidence. The supplied Monolith guide has been used for a private review crop, and animation speed is now global rather than V29-specific; adaptive registry validation and assigned-session swaps remain open. The Wayland-owner loading path remains blocked by RB-0005. The source-preserving V29 display was reported good, but its runner was aborted before technical finalization; output 115 pacing remains unresolved. The Worldwall launch hook is staged but next-login behavior is not verified. CR-0004's physical incompatible-capability output remains future work and `not-run`; no universal hardware-compatibility claim is made.

## Start here

1. [Delivery contract](docs/delivery-contract.md) — scope, invariants, acceptance, and approval.
2. [Roadmap](docs/roadmap.md) — canonical stage order and state.
3. [Current status](docs/status.md) — evidence, next action, and pending decisions.
4. [Documentation map](docs/README.md) — workflows, templates, and reference material.
5. [S02 reviewer guide](docs/reviews/s02-reviewer-guide.md) — step-by-step acceptance walkthrough.
6. [S03 evidence](docs/evidence/2026-09-21-s03-indexed-scene.md) — accepted M1 decoder/resource/live evidence.
7. [Contributor instructions](AGENTS.md) — how to work safely and leave a useful handoff.

**Current action:** Complete the remaining approved [CR-0006 implementation gates](docs/changes/CR-0006-weather-adaptive-scene-selection.md) and [CR-0007 animation evidence](docs/changes/CR-0007-global-animation-speed-multiplier.md). The private Monolith crop is review-only. RB-0005 still blocks claiming the off-thread IPC/config loading gates. No adaptive config activation, Worldwall launch/restart, next-login test, network request, private operator-config edit, or commit was performed. See the [CR-0006 update](docs/updates/2026-09-23-cr0006-weather-adaptive-scenes.md), [roadmap](docs/roadmap.md), and [current status](docs/status.md).

## Build and run the S03 indexed-scene path

S00 pins Rust 1.94.0 and exact dependency versions in `Cargo.toml`/`Cargo.lock`.
The default binary connects to the current Wayland session, binds SCTK output
and layer-shell globals, waits for configure, creates a Vulkan wgpu surface on
the retained Background layer, and presents one static indexed synthetic scene. Set `WORLDWALL_SOLID_COLOR=1` to use the S02 solid-color diagnostic path.
Set `WORLDWALL_LAYER_ONLY=1` to retain the S01 layer-only diagnostic path. Set `WORLDWALL_CONFIG=/path/to/worldwall.toml` for authoritative FPS, registry, optional explicit `[location]` (latitude, longitude, IANA timezone), and optional weather configuration. Schema v1-v3 remain readable; schema v2 adds location/weather, v3 adds default-off `[weather].adaptive_scenes`, and v4 adds global `animation_speed_multiplier` (default `0.5`, range `0.0..=1.0`). The tracked [example config](examples/worldwall.toml) leaves location unset, weather disabled, and adaptive selection off; it contains no sample coordinates or timezone. Add location only from explicit operator-supplied values.

Without `WORLDWALL_CONFIG`, legacy `WORLDWALL_REGISTRY_DIR`, `WORLDWALL_FPS`, `WORLDWALL_LOCATION_*`, `WORLDWALL_UTC_OFFSET_MINUTES`, and `WORLDWALL_WEATHER*` inputs remain supported. When a config file is selected, those runtime values do not override it. `WORLDWALL_SCENE_DIR=/path/to/package` remains a separate fixed-scene source; with neither a package nor registry, the original synthetic reference scene is used. Add an optional `animation.toml` and contained `time_of_day.png` atlas for GPU effects. The global multiplier uniformly scales every animated palette effect while leaving civil/solar sampling independent. `time_of_day.toml` schema v1 uses civil seconds; schema v2 can opt into solar-event anchors, which require explicit configured location/timezone and fall back to the base palette when unavailable. No host timezone or location is inferred. Suspend/resume listens to systemd-logind when available and reports degraded support when it is missing. Clock/solar changes do not select scenes; weather is cache-only unless the valid schema-v3 opt-in permits a fresh same-family precipitation variant, never a family switch. Open-Meteo makes no requests unless explicitly enabled in config (or legacy environment mode without a config file); enabled coordinates are sent over HTTPS and are not logged or stored raw in the cache.

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo build --locked
cargo run --locked
```

The accepted animation schema and GPU layout are recorded in
[ADR-0004](docs/decisions/ADR-0004-s04-animation.md); civil-time v1 and the
solar-event v2 atlas contracts are recorded in [CR-0002](docs/changes/CR-0002-s06-time-of-day-palettes.md)
and [CR-0005](docs/changes/CR-0005-configured-location-weather-solar-lighting.md).
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
disconnects. S10 also builds `worldwallctl`, which controls a running instance
through the same-user `$XDG_RUNTIME_DIR/worldwall.sock` socket:

```sh
worldwallctl status
worldwallctl reload
worldwallctl reload-config
worldwallctl scene <id>
worldwallctl next
worldwallctl weather
```

Set `WORLDWALL_CONFIG` to a versioned config file when startup/config
transactions should control `fps`, `registry_dir`, explicit location/timezone,
and optional weather/adaptive selection. `reload-config` validates and applies
it atomically, never reselects a scene, and may start one bounded asynchronous
request only if Open-Meteo is explicitly enabled. See [IPC and configuration](docs/ipc.md) for
the framed protocol, permission policy, stable semantics, and examples.

For repeatable lifecycle testing without changing the outer desktop
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
