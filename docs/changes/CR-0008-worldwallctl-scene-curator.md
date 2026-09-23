# CR-0008 — Curated scene pools and `worldwallctl` GUI

- State: applied
- Proposer / date: Pi coding agent / 2026-09-23
- Decision owner: `simplysoup`
- Baseline / affected scope: approved delivery contract v0.7 → v0.8; §4 GUI-editor non-goal; post-M4 S12; WW-03, WW-04, WW-06, WW-07, WW-08, WW-10, WW-12. Existing S00–S11 acceptance records remain historical.
- Motivation / evidence: the user requested a GUI command in `worldwallctl`, then clarified that Worldwall should consume the pool/condition settings and that `worldwallctl next` must cycle only eligible scenes. The [HTML concept mockup](../mockups/canvascycle-scene-curator.html) demonstrates the requested layout. The CanvasCycle page has 35 selectable entries plus a commented test placeholder; entries contain complete indexed images, palettes, and cycle data, not always palette-only variants. A private ignored V25HEAT capture is at `fixtures/untracked/effectgames/canvascycle-v25heat/`; two V08 time variants remain browser-session artifacts. The current `SceneRegistry::select_next` filters registered scenes by time/weather/season compatibility in stable ID order, but it has no enabled-pool or arbitrary temperature-range filter and only sees the configured registry. `worldwall.toml` schema v4 rejects unknown keys. `worldwallctl` has no GUI dependency. GTK 4.22.5 and WebKit2GTK 2.52.6 are present on the inspected host only; no supported GUI dependency/API matrix is established.

## Exact delta

| Item | Before | Proposed after | Reason |
| --- | --- | --- | --- |
| Contract §4 / roadmap | A GUI editor is a non-goal; no S12 exists. | Add proposed post-M4 S12 for a local scene-pool curator plus opt-in runtime candidate filtering. Keep the approved M4/S00–S11 record intact. | The requested UI and pool behavior exceed the approved baseline. |
| `worldwallctl` | CLI status/control; `next` cycles compatible entries in the active registry. | Add `worldwallctl curator [--library DIR] [--rules FILE]`. The GUI edits pool and scene-availability rules. The existing `next` command retains its syntax but uses the configured eligible candidate set when the new pool config is enabled. | Put the editor behind the requested command and make its choices usable by Worldwall. |
| Pool/catalog boundary | The renderer consumes one validated registry; CR-0006 adaptive registries require complete family/band coverage. | Keep CanvasCycle packages in their own library, outside the CR-0006 adaptive registry. A separate, opt-in pool file references registered package sources and their complete world-plus-cycle assets. A pool has an enable switch; each scene/appearance has an enable switch and user-authored availability constraints. The current registry can be represented as a pool; no pool file means legacy behavior. | Allow whole pools to be toggled without weakening adaptive-registry validation or deleting assets. |
| Eligibility | Existing scene tags include time/weather/season; generic temperature intervals and pool state are absent. | Every non-manual scene-selection path uses the same eligibility filter: enabled pool, enabled scene, and all configured time, temperature-range, and weather conditions must match one current environment snapshot. This covers startup/full selection, explicit full-selection commands, resume, `next`, and the approved CR-0006 same-family weather-variant choice. Weather choices within a rule are OR'd; enabled criteria across categories are AND'd. An unknown required input does not match. Preserve stable cycling order for `next`. An empty candidate set never causes a hidden fallback: `next` is a no-op with a reason; startup/full selection fails closed; resume or a variant attempt retains the resident scene and reports no eligible alternative. Manual `scene ID` selection explicitly bypasses pool and availability rules, but still requires a known, valid, loadable scene. | Apply pool and scene constraints everywhere automatic selection already occurs while preserving an explicit manual override. |
| Runtime event behavior | Clock ticks and ordinary weather updates do not select, except the separately approved CR-0006 same-family fresh-weather path. `reload-config` does not select. | Pool/rule edits take effect through validated config reload but do not themselves change the active scene. Every later automatic selection evaluates the then-current time/weather/temperature snapshot. Existing triggers and CR-0006 behavior remain the only automatic triggers; the common pool/availability filter gates their candidates. Do not add clock/weather-triggered cross-scene selection. | Make pool eligibility universal without changing when Worldwall selects. |
| Config wiring | Runtime schema v4 has no pool-file reference and rejects unknown keys. | Schema v5 adds an optional `scene_pools_file` reference in `worldwall.toml`; the curator writes a distinct pool/rules sidecar. `reload-config` validates the main config, sidecar, and all referenced assets as one candidate, then commits atomically without selecting. The GUI may request this reload explicitly after save; it must not silently issue `next`. | Keep the primary config authoritative while making the new source opt-in and transactional. |
| Asset/license boundary | Artwork is not bundled; a catalog listing is not a license. | Store source bytes/derived packages locally with source URL, hash, and license status. No background web fetch, telemetry, location inference, or weather request. Do not commit, publish, or package artwork until rights review grants that use. | Preserve existing privacy and provenance rules. |

Approved config-reference shape (illustrative; the sidecar's final manifest schema is still an S12 gate and no runtime parser is implemented):

```toml
# Approved v0.8 target; current runtime remains schema v4 until S12 lands.
# Omission preserves legacy registry and selection behavior.
schema_version = 5
scene_pools_file = "<absolute path to scene-pools.toml>"
```

```toml
# Separate scene-pools.toml; the current runtime does not yet read this.
schema_version = 1

[[pool]]
id = "canvascycle"
enabled = true
scene_ids = ["V25HEAT"]

[[scene_rule]]
scene_id = "V25HEAT"
enabled = true
time_of_day = ["day"]
temperature_min_c = 28.0
temperature_max_c = 45.0
weather_any = ["clear", "cloudy"]
```

The sample values are mockup data, not recommended policy. The active weather/time inputs continue to come from already configured Worldwall environment services; pool rules must not enable a provider or infer location/timezone. The existing adaptive registry remains structurally separate. The user has clarified that pools and scene conditions constrain **all automatic selection**, while manual `scene ID` selection bypasses those constraints. S12-AC1 still needs to settle source-pool composition, how the filter composes with CR-0006 family/hysteresis behavior, and the diagnostic/exit response for startup with no eligible scene. The eligibility outcome itself is fixed: no ineligible fallback; startup reports selection unavailable and an already-running scene is retained for a no-candidate trigger. These are implementation details, not permission to add new selection triggers.

## Implementation plan and gates

This approval adds a new post-M4 stage, **S12 — Local scene-pool curator and automatic eligibility**. It depends on S04 animation, S05 package/selection, S06 time inputs, S07 weather snapshots, S08 resume selection, S10 config/CLI, and S11 release guidance. It does not reopen those historical acceptances or make S12 part of the existing M4 release.

1. **S12-AC1 — Selector policy and host spike.** Scope and acceptance authority are recorded in CR-0008/v0.8. Lock source-pool versus current-registry composition; interaction with `adaptive_scenes` family/hysteresis behavior; unknown weather/temperature behavior; and the diagnostic/exit representation of startup selection-unavailable. Apply the pool/scene filter to every automatic selection path; preserve `scene ID` as a manual override. Keep all existing event triggers unchanged—do not add clock/weather/config-reload selection. Compare an embedded local WebView/native GTK window with an external-browser launcher; select a host only after a clean locked build and Hyprland open/close/input/lifetime spike. Do not add an unreviewed dependency or unsafe lifetime shortcut.
2. **S12-AC2 — Isolated library and importer.** Define a versioned manifest with pool IDs, scene IDs, world/appearance relationships, attached animation groups, source titles/URLs/hashes, license state, and editable conditions. Import the 35 selectable CanvasCycle entries from explicit local source captures. Bound file/count/dimension/palette/cycle parsing; reject unsafe paths, duplicate IDs, overlapping/invalid cycles, and malformed values; parse legacy object-literal data as data, never evaluate JavaScript. Unsupported source facts remain unknown. Never write to or weaken the adaptive registry.
3. **S12-AC3 — Curator UI and command.** Add `worldwallctl curator` with `--library`/`--rules` overrides and XDG defaults. Provide a scrollable/searchable list with thumbnails, pool enable/disable controls that operate on complete world-plus-cycle packages, animated selected-scene preview, ToD scrubber where variants exist, and scene time/weather/temperature controls. Show which scenes are eligible under the preview snapshot and why. Launch/close must not require or terminate the wallpaper process.
4. **S12-AC4 — Runtime config integration.** Implement the optional schema-v5 sidecar reference. Validate the complete pool candidate off the Wayland owner; a successful `reload-config` atomically applies it without selecting, and any parse/load error retains the previous runtime config/pools. Pool toggles never delete local assets. No provider request is started by editing or applying rules. All automatic selectors consume the validated pool/eligibility state; manual `scene ID` remains the explicit override.
5. **S12-AC5 — Automatic selection filtering.** Under the agreed pool mode, build candidates from enabled sources/scenes and one current environment snapshot for every automatic selection path, including startup/full selection, explicit full-selection commands, resume, `next`, and the approved CR-0006 same-family weather-variant choice. Require all configured scene constraints to match. Test exact boundaries, unknown/unavailable weather and temperature, disabled pool/scene, current scene excluded when alternatives exist, deterministic `next` wrap/order, no-candidate behavior for each trigger, manual-ID override, and unchanged legacy behavior when the sidecar is absent. Verify time/weather/config reload do not add selection triggers; an already-approved selection trigger sees the latest snapshot.
6. **S12-AC6 — Preview fidelity.** Preview actual imported indexed pixels and authored palette cycles; compare representative output with the existing GPU reference path. Time scrubbing is injected preview state and may switch among preloaded complete appearance assets. It must not be described as runtime time-triggered selection. Keep full image/palette/metadata uploads out of the per-frame loop.
7. **S12-AC7 — Transaction, privacy, host, and release validation.** Save the sidecar atomically with same-user permissions, preserving the prior valid file on failure. Verify the GUI changes only its library/rules and requests config reload explicitly; it does not silently change the active scene. Verify manual scene selection is the only selection path that bypasses pool/availability filters. Do not log coordinates or weather request details. Test keyboard navigation, scrolling, missing/corrupt assets, invalid rules, launch with and without Worldwall running, close/reopen persistence, supported dependency packaging, and assigned Hyprland behavior. Document licensing limits and all not-run gates; a screenshot alone is not acceptance.

Acceptance separates parser/model tests, GUI interaction tests, GPU preview evidence, selector/environment tests, and assigned-session window lifecycle. The HTML mockup is design evidence only.

## Alternatives and impact

- **Do nothing:** retain the mockup and leave `worldwallctl`, runtime selection, and contract unchanged.
- **GUI-only sidecar not consumed by Worldwall:** lower runtime risk, but does not satisfy the clarified requirement that pool and availability toggles affect runtime scene selection.
- **Approved opt-in pool mode:** keep package files separate from the adaptive registry, let `worldwallctl curator` author a sidecar, and have every automatic selection path use enabled/eligible entries; manual `scene ID` remains an override. When no sidecar is configured, behavior remains unchanged. Requires selector/config/CLI changes and a GUI host dependency decision.
- **Automatically swap scenes on clock/weather changes:** excluded. It would change WW-06/S06 trigger semantics and is unnecessary for `next` to evaluate the current snapshot.

The mockup uses synthetic art, candidate labels, and simulated controls. CanvasCycle assets are not licensed for redistribution based on evidence seen so far. Temperature/weather values remain user-authored criteria, not metadata inferred from the source title. A private local library does not imply permission to distribute assets or bundle them in a release.

Rollback disables/removes the optional pool-file reference and curator command; with no configured pool file, the current registry/selector behavior remains. Do not rewrite accepted S05/S06/S07/S10 evidence. Pool filtering does not add selection triggers: any later automatic environment-triggered selection policy beyond the existing contract needs a separate approved change if not covered by the final approved S12 scope.

## Approval

- Decision / conditions: Approved by `simplysoup` on 2026-09-23 without additional conditions. The user clarified that pool and availability filters apply to every automatic selection path, with manual `scene <id>` as the sole override; existing event triggers remain unchanged.
- Approver identity / date: `simplysoup`, 2026-09-23.

## Application

- Applying revision / evidence: [CR-0008 baseline application record](../updates/2026-09-23-cr0008-baseline-application.md); commit identity will be recorded after the baseline application commit.
- Updated contract version / files: v0.8; delivery contract, S12 stage, roadmap, status, architecture, IPC/config reference, scene-format reference, change register, curator mockup, and application record.
- Accepted evidence made stale: none. No runtime/software stage was implemented or accepted; S00–S11 records remain historical.
- Linked roadblock disposition: none.
