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
{"version":1,"command":"adaptive","mode":"status"}
{"version":1,"command":"adaptive","mode":"on"}
{"version":1,"command":"adaptive","mode":"off"}
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
    "selection_generation": 2,
    "adaptive_scenes": true
  }
}
```

`status` reports output IDs, active resident scene, weather state/freshness,
FPS, backend, degraded outputs, selection generation, and the effective
`adaptive_scenes` mode. It does not report coordinates, credentials, or weather
request URLs. `weather` returns only the same cache/freshness view and never
starts a provider request.

`adaptive status` reports the effective process-local mode in `data.mode` (`on`
or `off`) and `data.adaptive_scenes`. `adaptive on` and `adaptive off` are
idempotent: a successful change returns `applied: true`, `reselected: false`,
and the effective mode; an already-effective request returns
`outcome: "unchanged"` and does no source preparation. Enabling validates the
current Open-Meteo provider, explicit location/timezone, and complete adaptive
registry. Disabling leaves the weather cache/provider configured but disables
adaptive family/precipitation selection. With schema-v5 pool mode, disabling
adds the validated library packages to the non-adaptive automatic candidate
set, filtered by the same eligibility gate; enabling returns them to
manual-only status. The request prepares and validates the complete source set
off the Wayland owner, then commits mode and sources atomically. Errors use the
`adaptive_mode_rejected` code and retain previous mode/sources. Neither change
selects a scene, modifies pool rules/config, or starts a provider request. The
resident scene and selection generation remain unchanged. The command is
process-local; restart or a successful `reload-config` restores the file's
`[weather].adaptive_scenes` value.

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
resident scene. Reload also restores the effective adaptive mode from the file;
it remains non-selecting.

## Configuration file

Set `WORLDWALL_CONFIG` to a regular, non-symlink file. It is loaded at startup
and by `reload-config`. The current implementation reads schemas v1–v5: v2
adds optional location and weather tables; v3 adds the default-off adaptive
switch inside `[weather]`; v4 adds a global animation-speed scalar; v5 adds an
optional `scene_pools_file` sidecar reference (below). A minimal privacy-safe
v4 file is:

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

### Schema-v5 pool sidecar reference (S12-AC4)

Schema v5 adds optional `scene_pools_file`, an absolute path to a separately
versioned pool/availability sidecar (see [scene-pools](scene-pools.md)). If
absent, the existing registry and selection behavior is unchanged. When
present, every automatic selection uses only enabled pools and scenes whose
configured time/weather/temperature constraints match one current snapshot; a
constrained unknown input is not a match. Manual `scene <id>` explicitly
bypasses those filters but still validates/loads a known scene.
Applying/reloading this configuration never selects by itself, and it adds no
clock/weather triggers. If no scene is eligible, startup reports selection
unavailable and exits nonzero; later selection attempts keep the resident scene
or return a no-op reason instead of using a disallowed fallback. With a
sidecar configured, a `next` no-op is a successful `{"applied": false,
"outcome": "noop", "reason": "…"}` response (for example `no eligible scene for
next`); without a sidecar the historical response shapes are unchanged. The
`worldwallctl curator` command is a local GUI and does not add an IPC command;
it edits the sidecar and requests `reload-config` only on explicit user action.

## CLI

Control commands send the framed protocol and print JSON. `scenes` reads the
configured local registry and marks the active scene when the running Worldwall
instance is reachable:

```sh
worldwallctl status
worldwallctl scenes
worldwallctl reload
worldwallctl reload-config
worldwallctl scene v29-seascape
worldwallctl next
worldwallctl weather
worldwallctl adaptive status
worldwallctl adaptive on
worldwallctl adaptive off
```

Use `--socket PATH` only for a deliberately selected same-user test socket.
For `scenes`, this socket is used only to query the active-scene marker; the
scene IDs are read from the configured registry. Control commands return a
nonzero exit status when the service returns an error. `scenes` lists the
configured registry and the curator library catalog together, marking the
active scene in either source.

S12 adds a separate `worldwallctl curator` GUI command for editing the pool
sidecar. It browses and searches the local catalog with thumbnails, previews
indexed pixels and authored palette cycles through a WebGL2 port of the GPU
shader, scrubs known time-of-day variants as injected preview state, explains
per-scene eligibility under one preview snapshot, and edits pool enable/disable
plus per-scene time/weather/temperature rules. Saves are atomic with same-user
permissions and preserve the previous valid file on failure; *Request config
reload* is the only Worldwall contact and is never automatic. The curator is
not a socket/IPC command.
