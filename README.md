# Worldwall

A planned GPU-accelerated, weather- and time-aware pixel-art wallpaper engine for Hyprland/Wayland.

**Current state:** S00 foundation, S01 single-output Wayland lifecycle, S02 safe GPU presentation, and S03 indexed-scene/M1 are accepted. The S03 binary path decodes bounded 8-bit index PNGs, uploads `R8Uint` indices and an opaque palette, and presents a synthetic indexed scene on the retained Background layer.

## Start here

1. [Delivery contract](docs/delivery-contract.md) — scope, invariants, acceptance, and approval.
2. [Roadmap](docs/roadmap.md) — canonical stage order and state.
3. [Current status](docs/status.md) — evidence, next action, and pending decisions.
4. [Documentation map](docs/README.md) — workflows, templates, and reference material.
5. [S02 reviewer guide](docs/reviews/s02-reviewer-guide.md) — step-by-step acceptance walkthrough.
6. [S03 evidence](docs/evidence/2026-09-21-s03-indexed-scene.md) — accepted M1 decoder/resource/live evidence.
7. [Contributor instructions](AGENTS.md) — how to work safely and leave a useful handoff.

**Next action:** preserve the accepted S03/M1 baseline and close the S04 animation schema/GPU-packing decision before implementing animation. Do not build the entire proposed module tree up front.

## Build and run the S03 indexed-scene path

S00 pins Rust 1.94.0 and exact dependency versions in `Cargo.toml`/`Cargo.lock`.
The default binary connects to the current Wayland session, binds SCTK output
and layer-shell globals, waits for configure, creates a Vulkan wgpu surface on
the retained Background layer, and presents one static indexed synthetic scene. Set `WORLDWALL_SOLID_COLOR=1` to use the S02 solid-color diagnostic path.
Set `WORLDWALL_LAYER_ONLY=1` to retain the S01 layer-only diagnostic path. Set `WORLDWALL_SCENE_DIR=/path/to/package` to load `scene.png` and `palette.png`; without it, the original synthetic reference scene is used.

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo build --locked
cargo run --locked
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
