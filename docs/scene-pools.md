# Scene-pool library catalog and rules sidecar (S12)

This document defines the two versioned local file formats added for S12 under
[CR-0008](changes/CR-0008-worldwallctl-scene-curator.md): a curator library
catalog (`library.toml`) and a separately stored pool/scene-rules sidecar
(`scene-pools.toml`). They implement the [approved AC1 policy](updates/2026-09-23-s12-ac1-kickoff.md).
Both formats are validated offline by
[`tools/scene_pool_library.py`](../tools/README.md); runtime consumption of a
sidecar through the schema-v5 `scene_pools_file` reference is S12-AC4/AC5 work
and is not implemented. With no sidecar configured, existing registry and
selection behavior is unchanged.

Nothing here establishes an artwork license, authorizes redistribution, or
changes the CR-0006 adaptive registry contract. Imported CanvasCycle packages
stay outside the adaptive registry; their source-native cycle records remain
unmapped metadata, not animation data.

## Library root and catalog (`library.toml`, schema v1)

A library root holds one directory per complete package plus an explicit
catalog. There is no implicit directory discovery at consumption time: the
catalog is the authoritative list.

```text
<library>/
├── library.toml
├── canvascycle-v25heat/
│   ├── scene.toml
│   ├── scene.png
│   ├── palette.png
│   ├── provenance.toml
│   └── source-cycles.toml
└── ...
```

```toml
schema_version = 1

[[scene]]
id = "canvascycle-v25heat"
package = "canvascycle-v25heat"
source_collection = "canvascycle"
world = "unknown"
appearance = "unknown"
image = "scene.png"
image_sha256 = "<64 lowercase hex>"
palette = "palette.png"
palette_sha256 = "<64 lowercase hex>"
source_cycles = "source-cycles.toml"
source_cycles_sha256 = "<64 lowercase hex>"
cycle_semantics = "unmapped-source-native"
source_format = "canvascycle-legacy-object-literal-v1"
source_filename = "V25HEAT.LBM"
source_sha256 = "<64 lowercase hex>"
source_url = "https://example.invalid/capture"
source_page = "https://example.invalid/"
source_title = "Exact source title"
license_state = "unreviewed"
```

Catalog semantics:

- `schema_version` is `1`. `[[scene]]` entries are explicit; duplicate `id`
  values are rejected anywhere in the catalog.
- `id` is a stable local scene ID: 1–64 characters of `a–z`, `0–9`, `-`,
  starting with a letter or digit. It is never derived from a source title.
- `package` is a relative path inside the library root. Absolute paths, `\`,
  `.`/`..` components, and symlinked path components are rejected; the
  resolved directory must remain inside the library root.
- `source_collection` names the source set the package was imported from
  (for example `canvascycle`). It is derived from the validated `source_format`
  identifier by a fixed mapping, not inferred from titles or content.
- `world` and `appearance` record appearance relationships. When a
  relationship is not established by evidence, the value is the literal
  `unknown`; it is never guessed from filenames or titles. `appearance`
  requires a known `world`. These two fields are curator-authored and are
  preserved across catalog regeneration.
- Assets: `image` and `palette` are required. `animation`, `time_of_day`
  (with `time_of_day_atlas`), and `source_cycles` are optional. Each declared
  asset has a sibling `<key>_sha256` digest that must match the file.
  `animation` and `source_cycles` are mutually exclusive: unmapped source
  cycles are not animation data.
- `cycle_semantics` is required with `source_cycles` and currently accepts
  only `unmapped-source-native`.
- Provenance: `source_format`, `source_sha256`, `source_url`, `source_page`,
  `source_title`, and `license_state` are required; `source_filename` is
  optional. These values must exactly match the package's `provenance.toml`.
- `license_state` is one of `unreviewed`, `reviewed-granted`,
  `reviewed-withheld`. `reviewed-granted` additionally requires a non-empty
  `license_id` naming the reviewed grant; `license_id` is forbidden for the
  other states. Catalog generation always records the package's declared
  state and never upgrades it.

Each package directory contains exactly its declared assets plus `scene.toml`
and `provenance.toml`; unlisted files are rejected. `scene.toml` must declare
the same `id`, `image`, `palette`, and optional `animation`/`time_of_day`
references as the catalog. Scene-tag and CR-0006 `family`/`temperature_band`
keys may appear in `scene.toml` but are not interpreted by, copied into, or
validated against the catalog: adaptive registry validation is unchanged.

## Pool/rules sidecar (`scene-pools.toml`, schema v1)

The sidecar is a distinct, separately versioned file referenced only by the
optional schema-v5 `scene_pools_file` config key (S12-AC4). It holds pool
membership and the editable per-scene criteria; it never references files by
path and never enables a weather provider or infers a location.

```toml
schema_version = 1

[[pool]]
id = "canvascycle"
source = "library"
enabled = true
scene_ids = ["canvascycle-v25heat"]

[[scene_rule]]
scene_id = "canvascycle-v25heat"
enabled = true
time_of_day = ["day"]
temperature_min_c = 28.0
temperature_max_c = 45.0
weather_any = ["clear", "cloudy"]
```

Sidecar semantics:

- `schema_version` is `1`. At least one `[[pool]]` is required; `[[scene_rule]]`
  entries are optional. Unknown keys are rejected everywhere.
- Pool `id` is unique and ID-shaped. `source` is `library` or `registry`:
  `library` pools name catalog packages, `registry` pools name packages of the
  configured Worldwall registry (resolved by the runtime). Duplicate scene
  membership across pools — including across sources — is rejected, so a
  package ID can never be claimed twice.
- Pool `enabled` is required. Disabling a pool never deletes or rewrites
  package assets.
- A `[[scene_rule]]` names a scene that belongs to some pool and at most one
  rule exists per scene. `enabled` is required. Criteria keys are optional;
  a present criterion is configured and constrains matching:
  - `time_of_day`: non-empty array of distinct values from
    `dawn`, `day`, `dusk`, `night`, `polar_day`, `polar_night`.
  - `weather_any`: non-empty array of distinct values from
    `clear`, `cloudy`, `rain`, `snow`, `storm`, `fog`.
  - `temperature_min_c`, `temperature_max_c`: finite numbers in `-100..=100`;
    `min` must not exceed `max`.
- Criterion values exclude `unknown` deliberately: a configured criterion
  requires a known input and never matches an unknown environment value. An
  omitted criterion is unconstrained.

## Eligibility semantics (target for the S12-AC5 filter)

These rules define the shared filter for every automatic selection path; the
runtime filter is not implemented yet.

1. A candidate is eligible when its pool is enabled, the scene itself is
   enabled, and every configured criterion matches one current environment
   snapshot. Weather values inside one rule are alternatives (OR); enabled
   categories combine with AND.
2. A configured criterion with an unknown required input does not match; the
   candidate is not eligible.
3. The filter applies to every automatic selection trigger — startup/full
   selection, resume, explicit `reload`, `next`, and the approved CR-0006
   same-family weather-variant replacement — and adds no new trigger.
   `reload-config`, clock/solar ticks, ordinary weather refreshes, resize, and
   hotplug never select a scene.
4. CR-0006 composition is unchanged: the prospective temperature band/family
   is computed first, then the family's precipitation variants are filtered in
   the existing appearance-preference order. A rejected candidate never
   crosses to another family/band, never falls back to V29, and never mutates
   stored family/band state. Non-adaptive ranking runs the existing
   deterministic order over eligible candidates only; the designated fallback
   is selectable only inside the eligible set.
5. An empty eligible set never causes an ineligible fallback. Startup reports
   selection unavailable and exits nonzero before presentation with a bounded
   diagnostic such as `startup selection unavailable: no eligible scene`
   (no coordinates or raw weather details); `next` is a no-op with a reason;
   other triggers retain the resident scene.
6. Manual `scene <id>` bypasses pool and availability rules but still requires
   a known, valid, loadable package. `next` cycles eligible entries in stable
   order with wrap.

## Validation bounds and failure behavior

`tools/scene_pool_library.py` enforces these bounds and failure rules:

- Files are read only from explicitly supplied local paths. No network access,
  no code execution, and no writes outside `--write-catalog`.
- Per-file size cap 16 MiB; at most 256 packages and 32 files per package;
  at most 256 source cycles per package; scene images are capped at 16
  Mi-pixels. The scene PNG must be 8-bit grayscale or indexed, and the palette
  PNG must be a 256 × 1 8-bit opaque RGBA image.
- Asset references are simple relative names without `.`/`..` components or
  symlinks. `provenance.toml` URLs must be HTTPS without user information.
- Source-cycles files are validated strictly: bounded integer fields, ranges
  with at least two palette indices, and non-overlapping cycle ranges.
  `animation.toml` and `time_of_day.toml` receive structural validation
  (parseable, known `schema_version`, non-empty payload); their full semantic
  validation remains the accepted runtime loader's gate.
- Every malformed, duplicate, or path-escaping case fails before any write.
  `--write-catalog` builds and validates the complete catalog first, then
  replaces `library.toml` atomically; a failure preserves the previous file.

## Limitations and non-goals

The runtime does not read these files yet; schema-v5 config wiring and the
shared selector filter remain later S12 gates. The S12-AC3 curator defaults are
`$XDG_DATA_HOME/worldwall/scene-library` (fallback
`$HOME/.local/share/worldwall/scene-library`) and
`$XDG_CONFIG_HOME/worldwall/scene-pools.toml` (fallback
`$HOME/.config/worldwall/scene-pools.toml`). Relative XDG base values are
ignored in favor of the fallback, consistent with the XDG specification;
`--library DIR` and `--rules FILE` override the respective paths. A missing
default rules file means no sidecar is present, but an explicitly requested
missing rules file is an error.

The initial `worldwallctl curator` implementation is a read-only Firefox
catalog/preview slice: it displays actual catalog metadata, provides
searchable/scrollable scene selection, decodes the selected indexed image and
palette for a WebGL2 static preview, and summarizes an existing sidecar without
evaluating it. It serves only token-scoped data on an ephemeral loopback port
and uses a temporary isolated Firefox profile; it does not use Worldwall IPC.
Pool/scene controls, rule editing, eligibility explanations, authored
animation, and ToD preview remain unimplemented. The curator checks catalog
structure, contained non-symlink package/asset paths, and selected PNG data,
but it does not replace the full offline validator's digest/provenance/rule
validation (`tools/scene_pool_library.py`). Imported CanvasCycle cycle
rate/direction semantics remain unmapped, and no capture carries a reviewed
license. Registry-sourced pool membership is resolved by the runtime, not by
the offline validator.
