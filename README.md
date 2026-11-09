# Worldwall

A GPU-accelerated, weather- and time-aware pixel-art wallpaper engine for Hyprland/Wayland, written in Rust on wgpu and Smithay.

Worldwall presents indexed pixel-art scenes on the Wayland background layer, with palette animation, time-of-day and solar palettes, optional weather-adaptive scene selection, and a control socket for a companion CLI (`worldwallctl`). It does not reselect a scene on an ordinary clock tick, weather poll, output hotplug, or config reload — selection changes only through an explicit scene choice, an adaptive-mode change, or a configured reload.

## Demo

The curator preview, cycling the same scene through time of day and weather:

<video src="docs/demo/curator-preview.mp4" width="100%" controls></video>

Scene shown: "May — Jungle Waterfall" by Mark Ferrari, as displayed in the [EffectGames demo](https://www.effectgames.com/demos/worlds/). The artwork is copyrighted and is shown here for demonstration only; the licensed version is available in [Living Worlds by Pixfabrik](https://pixfabrik.com/livingworlds/). Art assets are intentionally not bundled in this repository.

## Loading the EffectGames scenes yourself

The demo scenes are not shipped with this repo, but you can fetch them from the EffectGames demo site for your own local use and convert them with the tools already in this repo. Everything below stays on your machine under `fixtures/untracked/` (git-ignored): **do not commit or publish the fetched art.** Its license state is unreviewed; see the [licensing note in the scene format](docs/scene-format.md).

1. **Pick a scene from the catalog.** The catalog is at <https://www.effectgames.com/demos/worlds/scenes.js>. Each entry has a `month`, a script name (`scpt`), and a catalog name (`name`). The full inventory, with the exact source URL for every variant, is in [`fixtures/effectgames-reference-manifest.toml`](fixtures/effectgames-reference-manifest.toml) and in `tools/generate_effectgames_fixtures.py`.
2. **Download the scene script.** The URL pattern is `https://www.effectgames.com/demos/worlds/cache/<NAME>-<MONTH>-<SCPT>.js`. For the Jungle Waterfall clear variant shown above:
   ```sh
   curl -o fixtures/untracked/effectgames/v08-jungle-waterfall-clear-fetch.json \
     https://www.effectgames.com/demos/worlds/cache/V08-05May-MAYCLRSCPT.js
   ```
   Note the filename: `<package-id>-fetch.json` (for example `v08-jungle-waterfall-clear-fetch.json`).
3. **Wrap it in the browser-capture JSON format.** The generator expects a JSON file with the script contents under a `text` key, not the raw `.js` file:
   ```sh
   python3 -c "
   import json
   p = 'fixtures/untracked/effectgames/v08-jungle-waterfall-clear-fetch.json'
   open(p, 'w').write(json.dumps({'text': open(p).read()}))
   "
   ```
4. **Convert it to a scene package.**
   ```sh
   python3 tools/generate_effectgames_fixtures.py
   ```
   This writes `scene.png`, `palette.png`, `animation.toml`, and `time_of_day.toml` into `fixtures/untracked/effectgames/v08-jungle-waterfall--clear/`, plus a `registry.toml` and a `SOURCE.txt` provenance note. It will report the variants you did not fetch as missing; that is expected if you only fetched one.
5. **Point Worldwall at the registry.** The generated `registry.toml` lists all 21 catalog variants, so if you only fetched some, replace it with one listing only the scenes you have, with `fallback` set to one of them:
   ```toml
   schema_version = 1
   fallback = "v08-jungle-waterfall--clear"
   scenes = ["v08-jungle-waterfall--clear"]
   ```
   (Re-running the generator restores the full 21-scene list, so re-apply this afterwards if you re-run it.) Then run with the registry directory, in your Hyprland session:
   ```sh
   WORLDWALL_REGISTRY_DIR=$PWD/fixtures/untracked/effectgames cargo run --locked
   ```
   Or set `registry_dir` to that path in your `worldwall.toml` and run with `WORLDWALL_CONFIG`. The December scene (V12 Winter Manor) has no source available, so it cannot be fetched this way.

## Requirements

- A Wayland compositor with layer-shell support (developed against Hyprland)
- Rust 1.94.0 (see `rust-toolchain.toml`), with `rustfmt` and `clippy`
- Vulkan drivers for your GPU, e.g. on Arch: `sudo pacman -S wayland vulkan-icd-loader`
- Your own scene artwork: **art assets are intentionally not bundled in this repository.** Supply scenes in the [scene format](docs/scene-format.md), or run the built-in synthetic reference scene with no scene package configured.

## Start here

1. [Architecture](docs/architecture.md) — environment, selector, scene resources, and renderer.
2. [Scene format](docs/scene-format.md) — scene packages, palettes, and animation.
3. [IPC and configuration](docs/ipc.md) — config schema, framed protocol, and `worldwallctl`.
4. [Release and packaging](docs/release.md) — Arch package, session launcher, and runbook.
5. [Documentation map](docs/README.md) — workflows, templates, and reference material.
6. [Contributor instructions](AGENTS.md) — how to work safely and leave a useful handoff.

Development history, stage evidence, and open decisions live in [current status](docs/status.md) and the [roadmap](docs/roadmap.md), not in this README.

## Build and run the S03 indexed-scene path

S00 pins Rust 1.94.0 and exact dependency versions in `Cargo.toml`/`Cargo.lock`.
The default binary connects to the current Wayland session, binds SCTK output
and layer-shell globals, waits for configure, creates a Vulkan wgpu surface on
the retained Background layer, and presents one static indexed synthetic scene. Set `WORLDWALL_SOLID_COLOR=1` to use the S02 solid-color diagnostic path.
Set `WORLDWALL_LAYER_ONLY=1` to retain the S01 layer-only diagnostic path. Set `WORLDWALL_CONFIG=/path/to/worldwall.toml` for authoritative FPS, registry, optional explicit `[location]` (latitude, longitude, IANA timezone), and optional weather configuration. Schema v1-v3 remain readable; schema v2 adds location/weather, v3 adds default-off `[weather].adaptive_scenes`, and v4 adds global `animation_speed_multiplier` (default `0.5`, range `0.0..=1.0`). Schema v5 adds the optional scene-pool sidecar reference; `worldwallctl adaptive on|off` can temporarily override the configured adaptive mode for the running process, while `reload-config`/restart restore the file value. The tracked [example config](examples/worldwall.toml) leaves location unset, weather disabled, and adaptive selection off; it contains no sample coordinates or timezone. Add location only from explicit operator-supplied values.

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


Run `cargo run --locked` only in your Hyprland/Wayland session; it stays
alive until the layer closes, its selected output is removed, or the compositor
disconnects. S10 also builds `worldwallctl`, which controls a running instance
through the same-user `$XDG_RUNTIME_DIR/worldwall.sock` socket:

```sh
worldwallctl status
worldwallctl scenes
worldwallctl reload
worldwallctl reload-config
worldwallctl scene <id>
worldwallctl next
worldwallctl weather
worldwallctl adaptive status
worldwallctl adaptive on
worldwallctl adaptive off
```

For an x86_64 Arch package and transfer/install instructions, see the
[Arch/AUR package section](docs/release.md#arch-linux-package). Its package
archive is a local build artifact; the recipe is not published to the AUR.

The post-M4 curator is under development. Its current read-only slice can be
opened independently of Worldwall with an explicit local library, for example:

```sh
worldwallctl curator --library /path/to/scene-library
```

It uses `$XDG_DATA_HOME/worldwall/scene-library` (fallback
`$HOME/.local/share/worldwall/scene-library`) and
`$XDG_CONFIG_HOME/worldwall/scene-pools.toml` (fallback
`$HOME/.config/worldwall/scene-pools.toml`) by default. `--rules FILE` overrides
the sidecar path. The current Firefox UI supports search, scene browsing, a
static indexed-image/palette GPU preview, and a read-only sidecar summary; it
does not edit rules, evaluate eligibility, preview animation, or control a
running Worldwall process. See [scene-pool formats and current limits](docs/scene-pools.md).

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
trace evidence, not a replacement for live compositor acceptance. See the [S02
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
