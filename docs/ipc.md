# Local IPC and configuration

S10 exposes a bounded, same-user Unix-socket control service. The default
socket is `$XDG_RUNTIME_DIR/worldwall.sock` (normally
`/run/user/$UID/worldwall.sock`). The service validates the runtime directory,
uses mode `0600`, checks Linux peer credentials, bounds clients and queues, and
rejects unsafe or foreign stale paths. It never lets a socket thread touch
Wayland, wgpu, scene resources, or selection state; requests are handed to the
Wayland event-loop owner. Repeated event-loop command failures emit a bounded
`trace ipc-diagnostic` record immediately and then at most once per second per
error code, including the suppressed count but no path, coordinate, or
credential.

Scene/package decoding and replacement-config registry validation run on
bounded background workers. Startup config and scene validation use loader
threads before event-loop dispatch begins. At runtime, explicit scene requests,
adaptive scene work, and config validation use separate single-slot job and
completion channels; calloop wakeups deliver results even when no frame timer is
armed. The event-loop owner alone applies config, uploads resources, and commits
scene swaps. IPC commands remain FIFO: while a scene or config command awaits its
final result, later IPC requests stay queued, while Wayland dispatch, rendering,
power handling, and weather-cache updates continue. The protocol retains its
existing final success/error responses and two-second client timeout; it does
not return an accepted/pending response. Slow work can still outlast that
pre-existing client timeout, without blocking the Wayland owner.

## Protocol

Protocol version is `1`. Each request and response is one big-endian `u32`
length-prefixed UTF-8 JSON frame. Requests are limited to 4096 bytes,
responses to 16 KiB, and a command has a two-second end-to-end timeout. The
service accepts at most 16 simultaneous clients and queues at most 32 commands.
Malformed, oversized, unsupported-version, unauthorized, stalled, and
out-of-order input is rejected without changing runtime state.

Requests use these forms:

```json
{"version":1,"command":"status"}
{"version":1,"command":"reload"}
{"version":1,"command":"reload-config"}
{"version":1,"command":"scene","id":"v29-seascape"}
{"version":1,"command":"next"}
{"version":1,"command":"weather"}
```

Successful responses contain `version`, `ok: true`, `command`, and `data`.
Errors contain `version`, `ok: false`, and an `error` object with a stable
machine-readable `code` and bounded message. A representative status result
is:

```json
{
  "version": 1,
  "ok": true,
  "command": "status",
  "data": {
    "outputs": [106, 107],
    "scene": "v29-seascape",
    "weather": {"state": "disabled", "freshness": "unknown", "cache_epoch": 0},
    "fps": 30,
    "backend": "vulkan",
    "degraded_outputs": [],
    "selection_generation": 2
  }
}
```

`status` reports output IDs, active resident scene, weather state/freshness,
FPS, backend, degraded outputs, and selection generation. It does not report
coordinates, credentials, or weather request URLs. `weather` returns only the
same cache/freshness view and never starts a provider request.

`reload` performs one explicit environment snapshot and full selection when a
validated registry is active. With valid schema-v3 adaptive opt-in, startup,
resume, and `reload` use temperature bands and persisted 2°C hysteresis; a
fresh weather result may update only a precipitation variant in the active
family. It cannot change families. `scene ID` validates and explicitly selects
the requested registered scene. `next` selects the next compatible registered
scene, or returns a bounded rejection/no-op reason. An invalid candidate,
missing ID, failed asset load, or renderer upload leaves the previous resident
scene intact. A fixed package/synthetic startup has no registry selection and
returns `selection_unavailable` for selection commands.

`reload-config` validates a complete candidate and applies it without scene
selection. The old configuration and runtime remain active if parsing, schema,
path, registry, fallback, timezone, or provider setup fails. A valid change may
alter FPS, animation speed, registry, configured location/IANA timezone, or weather provider;
the resident scene is kept. Animation-speed changes preserve the current phase
and do not select a scene. Registry transitions validate the complete
replacement before commit. Switching to `registry_dir = "none"` disables future
registry selection while retaining the resident scene and its status identity;
switching back installs the validated registry without selecting. Weather is
disabled by default. A newly enabled Open-Meteo provider may start one bounded
asynchronous request after commit; disabling/reconfiguring weather retires the
old worker first. Clock/solar updates never select a scene. Without adaptive
opt-in, weather updates cache/status only. With valid opt-in, fresh known
weather may change only the active family's precipitation variant. A successful
config reload invalidates queued adaptive candidates without changing the
resident scene.

## Configuration file

Set `WORLDWALL_CONFIG` to a regular, non-symlink file. It is loaded at startup
and by `reload-config`. Schema v1 remains readable; schema v2 adds optional
location and weather tables; schema v3 adds the default-off adaptive-scene
switch inside `[weather]`; schema v4 adds a global animation-speed scalar. A
minimal privacy-safe v4 file is:

```toml
schema_version = 4
fps = 30
registry_dir = "none"
animation_speed_multiplier = 0.5

[weather]
provider = "disabled"
adaptive_scenes = false
```

The optional `[location]` table is all-or-nothing and requires finite bounded
`latitude`, `longitude`, and an explicit recognized IANA `timezone`. Omit it
until the operator supplies the intended values. The tracked example intentionally contains no location values. Configure
`latitude`, `longitude`, and `timezone` only after the operator supplies those
exact inputs; no sample coordinates or timezone are used as defaults.

`[weather]` is optional and defaults to disabled. `provider = "open-meteo"`
requires the complete location table, reuses its coordinates, and enables
bounded asynchronous HTTPS requests. In schema v3 and later, `adaptive_scenes = true`
additionally requires Open-Meteo, explicit location, and a validated adaptive
registry. The opt-in applies the approved family/band mapping and 2°C
hysteresis to full-selection triggers, and permits only fresh same-family
precipitation variants between them. It defaults to false. Schema v4 adds the
top-level `animation_speed_multiplier` in the inclusive range `0.0..=1.0`;
`0.5` is the default, `0.0` pauses palette animation, and `1.0` uses authored
speed. Missing values in older schemas also default to `0.5`; schema-v1/v2
files remain readable. The multiplier applies to every animated palette effect,
not to civil-time/solar sampling. A successful config reload never selects and
invalidates queued adaptive candidates. Each adaptive family requires a clear
variant, all five bands must be represented, and clear `v29-seascape` is the
offline fallback. Only `fps = 15` or `fps = 30` is accepted. Registry loading validates
every listed package and its declared fallback before a config candidate can
commit. A selected config file is authoritative for FPS, registry,
location/timezone, and provider; legacy environment inputs are read only when
`WORLDWALL_CONFIG` is unset. The environment variables
`WORLDWALL_SCENE_DIR` and diagnostics remain separate scene-source/test inputs.

## CLI

The installed/local client sends the framed protocol and prints JSON:

```sh
worldwallctl status
worldwallctl reload
worldwallctl reload-config
worldwallctl scene v29-seascape
worldwallctl next
worldwallctl weather
```

Use `--socket PATH` only for a deliberately selected same-user test socket.
The client returns a nonzero exit status when the service returns an error.
