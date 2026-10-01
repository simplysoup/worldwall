# Worldwall for Arch Linux

Worldwall is a foreground wallpaper process for a Hyprland/Wayland session. It
is not a system service: start it as your desktop user, not as root.

## Start it

From an active Wayland session:

```sh
worldwall
```

To start it with Hyprland, add one entry to your existing Hyprland config:

```ini
exec-once = /usr/bin/worldwall
```

Choose one startup method; the package does not edit session configuration or
install a user service. The package installs `worldwallctl` on `PATH` for
controls, for example `worldwallctl status`, `worldwallctl scenes`,
`worldwallctl reload`, `worldwallctl next`, or `worldwallctl weather`.

## Configuration and scenes

Worldwall does not install a private config or bundle scene artwork. It can
start without a config, but then uses its built-in synthetic reference scene.
For a starting config, copy the privacy-safe example:

```sh
install -Dm600 /usr/share/doc/worldwall/examples/worldwall.toml \
  "${XDG_CONFIG_HOME:-$HOME/.config}/worldwall/worldwall.toml"
```

The example contains `registry_dir = "none"` and selects no artwork. Edit the
config to point `registry_dir` at a validated local scene registry, then launch
with:

```sh
WORLDWALL_CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/worldwall/worldwall.toml" worldwall
```

Only use scene packages whose provenance and license permit your use. Weather
is disabled by default; enabling it requires explicit operator configuration
and location values. No location or artwork is inferred or downloaded by the
package.

A Vulkan ICD/driver appropriate for the GPU is required in addition to the
package's declared `wayland` and `vulkan-icd-loader` dependencies. See the
project's release and IPC documentation in the source repository for detailed
configuration, privacy, recovery, and control semantics.
