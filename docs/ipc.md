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

`reload` performs one explicit environment snapshot and selection when a
validated registry is active. `scene ID` validates and explicitly selects the
requested registered scene. `next` selects the next compatible registered
scene, or returns a bounded rejection/no-op reason. An invalid candidate,
missing ID, failed asset load, or renderer upload leaves the previous scene and
selection generation unchanged. A fixed package/synthetic startup has no
registry selection and returns `selection_unavailable` for selection commands.

`reload-config` validates a complete candidate and applies it without scene
selection. The old configuration remains active if parsing, schema, path,
registry, or fallback validation fails. A valid change may alter the FPS target
and registry used by future explicit selections; the resident scene is kept. A
registry path transition validates the complete replacement before commit.
Switching to `registry_dir = "none"` disables future registry selection while
retaining the resident scene and its status identity; switching back installs
the validated registry without selecting. The command does not fetch weather
or reselect a scene.

## Configuration file

Set `WORLDWALL_CONFIG` to a regular, non-symlink file. It is loaded at startup
and by `reload-config`:

```text
schema_version = 1
fps = 30
registry_dir = "/home/user/.local/share/worldwall/registry"
```

Use `registry_dir = "none"` for a synthetic/fixed-scene configuration. Only
`fps = 15` or `fps = 30` is accepted. Registry loading validates every listed
package and its declared fallback before a config candidate can commit.
Without `WORLDWALL_CONFIG`, the legacy explicit environment inputs remain
available (`WORLDWALL_FPS` and `WORLDWALL_REGISTRY_DIR`).

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
