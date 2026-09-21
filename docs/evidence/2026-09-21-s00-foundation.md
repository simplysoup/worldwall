# Evidence — S00 foundation increment

- Date / operator: 2026-09-21 / Pi coding agent
- Revision or artifact hashes: local Git commit `85f2952` (parent `6023377` from `origin/main`); SHA-256 prefixes are recorded below for stable scaffold files
- Stage / requirements: S00; WW-12; S00-AC1 and S00-AC4 licensing/ownership increment
- Environment: Linux x86_64, kernel `7.2.4-zen2-1-zen`, Rust `1.94.0`, Cargo `1.94.0`, Wayland client library `1.26.0`, Vulkan instance `1.4.357`, NVIDIA GeForce RTX 3070 Ti, NVIDIA driver `615.71.09`; active session reports Hyprland and `WAYLAND_DISPLAY=wayland-1`
- Inputs: no scene or network input; the binary is a compositor-free dependency smoke target
- Kind: build/API spike and documentation

## Results

| Criterion | Exact command or manual procedure | Expected | Observed | Outcome | Artifact |
| --- | --- | --- | --- | --- | --- |
| S00-AC2 (format/lint/test/build slice) | `cargo fmt --check`; `cargo clippy --locked --all-targets --all-features -- -D warnings`; `cargo test --locked --all-targets --all-features`; `cargo build --locked`; `cargo run --locked` | Locked Rust target compiles, checks run, and the smoke binary executes | All commands passed; one test passed and the binary printed `worldwall foundation scaffold` | pass | `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `src/main.rs` |
| S00-AC2 (clean-checkout slice) | `git clone --no-local . <temporary-directory>` followed by `cargo fmt --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked --all-targets --all-features`, `cargo build --locked`, `cargo run --locked`, and both Python checks | A checkout from the recorded commit can reproduce the scaffold | Clean `main` checkout passed all Rust and documentation commands; smoke binary printed `worldwall foundation scaffold` | pass | local Git commit `1e496b8` and temporary clone (removed after validation) |
| S00-AC3 (dependency/API slice) | `cargo info` for `smithay-client-toolkit`, `wgpu`, `raw-window-handle`, `wayland-client`, `calloop`, and `calloop-wayland-source`; locked compile of `src/main.rs` | Selected versions expose a feasible SCTK/calloop/Wayland/raw-handle/wgpu Vulkan path | All pinned crates resolved and the API smoke target compiled; no raw handle or `'static` lifetime was constructed | pass | `Cargo.toml`, `Cargo.lock`, `src/main.rs` |
| S00-AC4 (environment inventory slice) | `uname -a`; `vulkaninfo --summary`; `hyprctl version`; `hyprctl monitors -j`; session/process inventory | Distinguish available local capabilities from assigned acceptance host/operator | Local Hyprland 0.56.2, Vulkan 1.4.357, NVIDIA RTX 3070 Ti/615.71.09, and two 1920×1080 scale-1 outputs were observed; operator and acceptance assignment remain absent | blocked | command output retained in session; no hardware acceptance claimed |
| S00-AC1 (authority and baseline ratification slice) | Maintainer instruction recorded in this increment; inspect ADR-0001 fields | Named maintainer/reviewer and recorded approval/date | `simplysoup` ratified ADR-0001 without amendment on 2026-09-21 | pass | `docs/decisions/ADR-0001-delivery-baseline.md`, `docs/delivery-contract.md` |
| S00-AC4 (licensing/provenance slice) | Inspect `LICENSE`, `Cargo.toml`, and `fixtures/README.md` | Code/artwork licensing and fixture provenance are explicit and lawful | Project code/docs use MIT; no art assets or generated fixture packages are bundled or committed; synthetic fixture provenance is documented | pass | `LICENSE`, `Cargo.toml`, `fixtures/README.md` |
| S00-AC5 (documentation-check slice) | `python3 tools/check_docs.py`; `python3 -m unittest discover -s tools/tests -v` | Documentation structure and checker tests pass | Checker reported `PASS: 59 Markdown files, 12 stages, 7 skills; archive intact`; 33 tests passed | pass | command output retained in session; `tools/check_docs.py` |
| Artifact identity | `sha256sum Cargo.toml Cargo.lock rust-toolchain.toml src/main.rs fixtures/README.md LICENSE` | Stable hashes identify the increment without Git | `Cargo.toml` `d3782f10`; `Cargo.lock` `e6cd8920`; `rust-toolchain.toml` `f2e51a6a`; `src/main.rs` `3934e3fd`; `fixtures/README.md` `e11491c4`; `LICENSE` `6885b07a` | pass | working-tree files

## Dependency/API record

The manifest uses exact versions: SCTK `0.21.1` with its `calloop` feature,
calloop `0.14.4`, calloop-wayland-source `0.4.1`, wayland-client `0.31.15`
with the system backend, raw-window-handle `0.6.2`, and wgpu `30.0.1` with
`std`, `vulkan`, and `wgsl`. Cargo metadata reports SCTK MSRV 1.86, wgpu MSRV
1.87, and the pinned project toolchain is Rust 1.94.0. These are a compile
baseline, not an accepted device matrix or a proof of raw-handle lifetime
safety.

The current API spike deliberately constructs no display, surface, or raw
handle. An initial compile attempt incorrectly assumed a `client::Display`
type; the pinned wayland-client API instead exposes `Connection`, and
`WaylandSource` is parameterized by event-loop data. The target was corrected
to compile against `Connection` and `WaylandSource<()>`; the compiler output is
retained as the API-selection evidence for this increment.

The future ownership boundary is: SCTK owns the connection, registry, outputs,
layer-shell role, and `wl_surface`; the dependent wgpu surface is created only
after configure and remains dropped before those Wayland owners. No `'static`
lifetime is manufactured. S02 must replace this sketch with a reviewed
implementation and evidence for pointer validity, thread affinity,
configure/ack ordering, frame callback ordering, and destruction order.

## Limitations and failures

The local repository now has Git history and origin ownership. A first
`git clone --local` attempt was blocked by a cross-device hard-link error;
`git clone --no-local` passed the complete locked build/check sequence. This
evidence does not prove visible presentation, GPU output, input pass-through,
teardown, power behavior, or performance. A local Hyprland/Vulkan session is
observed, but it is not an assigned acceptance host until an operator and
hardware matrix are recorded. The maintainer selected MIT for project
code/documentation and directed that art assets and generated fixture packages
stay out of the repository.

## Review and acceptance

- Reviewer / date: `simplysoup` / 2026-09-21
- Decision: ADR-0001 baseline ratified without amendment; S00 remains in progress and is not accepted
- Unmet criteria / stale dependent evidence: S00-AC4 live-host/operator assignment remains open; licensing/provenance and clean-checkout direction are recorded
- Next owner / action: `simplysoup` to name the live Hyprland/Vulkan operator, complete S00 review, and explicitly authorize any push
