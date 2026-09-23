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

For Hyprland's legacy text config, an operator may use
`exec-once = /absolute/path/to/worldwall` after testing that exact session
environment. For Lua configurations, use the opt-in launcher below. Choose
only one startup mechanism; no user-service unit is supplied.

## Opt-in Hyprland Lua session launcher

Only change a session configuration with the operator's permission. The source
checkout supplies [examples/worldwall-session.sh](../examples/worldwall-session.sh);
it is **not currently included by the release packager**. It requires a POSIX
shell and util-linux `flock`, an active user Wayland session, a readable private
config, and installed executable binaries. For the default XDG layout, install
from the checkout root after building/testing the release binaries:

```sh
install -d "$HOME/.local/bin" "$HOME/.local/share/worldwall/bin"
install -m 755 examples/worldwall-session.sh "$HOME/.local/bin/worldwall-session"
install -m 755 target/release/worldwall target/release/worldwallctl \
  "$HOME/.local/share/worldwall/bin/"
```

Do not overwrite a running executable; stop the operator-selected instance
before replacing binaries, or stage a new file and atomically rename it. The
launcher reads `${XDG_CONFIG_HOME:-$HOME/.config}/worldwall/worldwall.toml`
(or explicit `WORLDWALL_CONFIG`) and runs
`${XDG_DATA_HOME:-$HOME/.local/share}/worldwall/bin/worldwall`. Supply a validated
registry and config separately; installing the script does not configure
location, enable weather, or supply artwork. If using non-default XDG paths,
install files there and ensure Hyprland inherits those variables.

Back up the active Lua configuration, remove any competing wallpaper startup
and legacy `WORLDWALL_SCENE_DIR` override, and add this call to its **existing**
startup handler (replace the illustrative home path):

```lua
hl.on("hyprland.start", function()
    hl.exec_cmd("/home/YOUR_USER/.local/bin/worldwall-session")
end)
```

Do not add a second handler just to copy this example. Verify the active config
with `Hyprland --verify-config -c /absolute/path/to/hyprland.lua`. After an
operator-authorized config reload, inspect `hyprctl configerrors`. To test the
exact launch command without logging out, run:

```sh
hyprctl eval 'hl.exec_cmd("/home/YOUR_USER/.local/bin/worldwall-session")'
"$HOME/.local/share/worldwall/bin/worldwallctl" status
```

The inherited runtime lock makes repeated launcher invocations successful
no-ops; it does not coordinate other direct launches of `worldwall`. Never
unlink the lock while an instance is running. Worldwall itself subscribes to
logind for sleep/resume: **do not add a second resume launch hook**. A successful
logind connection is not a physical-resume test. This launcher is not a crash
supervisor; after diagnosing a fatal exit, invoke it again to restart.

Schema-v4 `animation_speed_multiplier = 0.5` plays authored animation at half
speed; `fps = 30` controls submission rate independently. Changing one does not
implicitly change the other. After editing the private config, apply it with:

```sh
"$HOME/.local/share/worldwall/bin/worldwallctl" reload-config
```

This updates valid configuration without selecting a new scene. Use `reload`
only when a full selection is intended. Normal stdout is discarded to avoid
unbounded per-frame logs; runtime stderr appends to
`${XDG_STATE_HOME:-$HOME/.local/state}/worldwall/session-errors.log`. Launch-time
preflight errors remain on the caller's stderr. For a **bounded diagnostic run**
with no existing instance, set `WORLDWALL_SESSION_TRACE=/private/path/trace.log`
and explicitly end the run afterward; it is not an automatically bounded or
rotated log. The launcher removes inherited diagnostic/test and fixed-scene
overrides so the config is authoritative. See the [assigned-host activation
record](updates/2026-09-23-hyprland-session-activation.md) for this deployment's
checks, backups, and untested reboot/resume gates.

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
