# Worldwall

A planned GPU-accelerated, weather- and time-aware pixel-art wallpaper engine for Hyprland/Wayland.

**Current state:** S00 foundation work has started with a minimal Rust/API smoke target. `simplysoup` is the assigned maintainer/reviewer; the delivery baseline is still review-ready pending explicit ratification. Code is MIT-licensed. No renderer, packaged artwork, runtime, or performance claim has been verified.

## Start here

1. [Delivery contract](docs/delivery-contract.md) — scope, invariants, acceptance, and approval.
2. [Roadmap](docs/roadmap.md) — canonical stage order and state.
3. [Current status](docs/status.md) — evidence, next action, and pending decisions.
4. [Documentation map](docs/README.md) — workflows, templates, and reference material.
5. [Contributor instructions](AGENTS.md) — how to work safely and leave a useful handoff.

**Next action:** have `simplysoup` ratify or amend the S00 baseline and assign the live-host operator. Prove a single-output static indexed image through S03 before adding environmental features. Do not build the entire proposed module tree up front.

## Build the foundation smoke target

S00 pins Rust 1.94.0 and exact dependency versions in `Cargo.toml`/`Cargo.lock`.
The binary only checks that the selected SCTK/calloop/Wayland/raw-window-handle/
wgpu Vulkan API path compiles; it does not connect to Wayland or claim renderer
acceptance.

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo build --locked
```

See the [S00 evidence record](docs/evidence/2026-09-21-s00-foundation.md) for
known limitations and pending approval/host assignments.

## Check the documentation

From the repository root, using Python 3.10 or newer:

```sh
python3 tools/check_docs.py
python3 -m unittest discover -s tools/tests -v
```

These commands validate documentation structure, not Worldwall's runtime. See [tooling conventions and limits](tools/README.md). The current application build/run instructions are the S00 smoke-target commands above; renderer and live compositor instructions will be added by their stages.

## Local developer skills

Project skills live in [`.agents/skills/`](.agents/skills/README.md). They support staged execution, evidence-based updates, roadblock diagnosis, roadmap changes, learning capture, delegation, and renderer review. Pi discovers these skills in a trusted project; other agents can read their `SKILL.md` files directly. No external packages or global configuration changes are required.

## History and licensing

The [original outline](docs/archive/project-outline.original.md) is preserved verbatim; it is historical, not a second specification. See the [audit](docs/audit.md) for corrections and proposed decisions. Project code and documentation are released under the [MIT License](LICENSE). Art assets are intentionally not bundled in this repository; do not redistribute unlicensed assets or infer permission from the Effect Games inspiration link.
