# Release-candidate runbook

This runbook describes the tracked release-candidate package produced by
`tools/package_release.py`. It does not publish artifacts, install a service,
or change a compositor/session configuration.

## Build an identifiable package

Run from a clean checkout with the locked toolchain:

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
python3 tools/check_docs.py
python3 -m unittest discover -s tools/tests -v
python3 tools/package_release.py --output-dir target/worldwall-release
```

The packaging command performs `cargo build --locked --release`, then writes
`target/worldwall-release/manifest.json`. The manifest records the package
version, source revision, dirty-tree state, every packaged file's size and
SHA-256, and a deterministic package digest. A clean tree is required by
default; `--allow-dirty` is only for a local smoke package and is recorded as
`source_dirty = true`. Use `--force` only when replacing this generated
`target/` directory.

The package contains `bin/worldwall`, `bin/worldwallctl`, the operator and
contract documentation, the MIT license, and `examples/worldwall.toml`. It
contains no scene fixtures, downloaded images, or other artwork. The manifest's
`bundled_artwork` list must remain empty for this repository's delivery
boundary.

## Foreground launch and controls

Copy the package directory to a location controlled by the operator and run
`bin/worldwall` from the active Hyprland/Wayland session. Do not launch it as
root. The layer is an input-free background and remains alive until shutdown,
compositor disconnect, or selected-output removal. A minimal configuration can
be supplied with `WORLDWALL_CONFIG=/path/to/worldwall.toml`.

With the package directory as the working directory, control a running process
using `bin/worldwallctl`:

```sh
bin/worldwallctl status
bin/worldwallctl reload
bin/worldwallctl reload-config
bin/worldwallctl scene SCENE_ID
bin/worldwallctl next
bin/worldwallctl weather
```

`worldwallctl` uses `$XDG_RUNTIME_DIR/worldwall.sock`; the socket is same-user
and mode `0600`. `reload-config` validates a complete candidate before commit
and does not reselect the resident scene. `weather` reports cache state only;
it does not enable networking. See [IPC and configuration](ipc.md) for the
full protocol and privacy behavior.

For Hyprland foreground startup, an operator may use `exec-once = /absolute/path/to/worldwall`
after testing that exact session environment. This is not a packaged service
and no user-service unit is supplied. Avoid configuring both mechanisms, which
would create duplicate instances.

## Recovery and troubleshooting

- If startup reports an unavailable Wayland/layer-shell/Vulkan dependency,
  run it in the intended Hyprland session and record the exact compositor,
  driver, Vulkan, and output versions. Do not call a headless run hardware
  evidence.
- If the socket is unavailable, confirm that the same user owns
  `$XDG_RUNTIME_DIR`, that no second instance is running, and that the runtime
  directory is not group/other writable. A stale same-user socket is removed
  safely; another user's path is rejected.
- Invalid scene/config/cache data is rejected without replacing the last valid
  resident scene. Inspect stderr and the structured IPC error before changing
  files.
- A surface timeout or outdated/lost result follows the bounded renderer path;
  repeated failures must remain visible in the trace. Device/OOM behavior is a
  release limitation unless the recorded matrix includes an observed host
  result.
- Suspend pauses deadlines and resume performs one coalesced environment
  refresh/selection. Missing logind is reported as degraded support, not
  treated as a fabricated resume.

The release package does not claim universal hardware compatibility, measured
power savings, driver allocator/VRAM statistics, or completed S11 acceptance.
Those claims require the approved matrix, numeric budgets, raw measurements,
and retained evidence described in [validation](validation.md).

## Operator configuration and privacy

Scene registries and scene packages are supplied separately. Art assets are not
bundled by this project; verify provenance and license terms before use. Weather
is disabled unless the operator explicitly selects `provider = "open-meteo"`
in a schema-v2-or-later config with a complete `[location]` table containing
explicit coordinates and an IANA timezone. The location table may also be used
for local solar calculations while weather stays disabled. Schema v3 adds
`[weather].adaptive_scenes`, default false; enabling it requires Open-Meteo and
a validated adaptive registry. It permits only fresh same-family precipitation
variant updates and startup/resume/full-selection family policy, never
cross-family changes from a weather poll. Do not enable it with a partial
registry or unresolved private artwork decisions. Enabling weather makes the
operator the Open-Meteo API consumer: the free API is for non-commercial use
only, is rate-limited by Open-Meteo (currently 10,000 calls/day, 5,000/hour,
600/minute), and its data is used under the CC-BY 4.0 licence, which requires
attribution to Open-Meteo. See [Open-Meteo's terms](https://open-meteo.com/en/terms)
for the current text; Worldwall's fixed polling cadence requests at most one
update per 900 seconds (about 96 calls/day) and sends coordinates only to that
provider over HTTPS. Coordinates/timezone are not logged or stored raw in the
weather cache. The cache is scoped to the selected config-file generation so a
configuration edit cannot reuse a prior location's snapshot. Host
timezone/location inference, telemetry, and automatic geolocation are not
performed.
