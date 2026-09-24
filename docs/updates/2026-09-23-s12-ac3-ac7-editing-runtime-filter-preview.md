# S12 AC3–AC7 increment: curator editing, schema-v5 runtime pools, shared selection filter, preview fidelity

**Stage:** S12 (post-M4 scene-pool curator and automatic eligibility) ·
**Requirements:** WW-03, WW-04, WW-06, WW-07, WW-08, WW-10, WW-12 ·
**Criteria covered by implementation evidence:** S12-AC3, S12-AC4, S12-AC5,
S12-AC6, S12-AC7 · **Owner:** Pi coding agent · **Baseline:** contract v0.8 with
CR-0008 applied.

This record documents implementation and test evidence only. It is **not**
acceptance: every AC gate below remains a maintainer decision, and the S12
stage stays `in-progress`. No artwork is bundled, published, or licensed by
this increment, and no Worldwall session configuration was changed.

## What changed

| Area | Files | Behavior |
| --- | --- | --- |
| Shared S12 model | `src/scene_pools.rs` (new) | One implementation of the accepted `library.toml`/`scene-pools.toml` schemas, the eligibility evaluation with bounded reasons, the deterministic sidecar writer, and the bounded path-safe catalog loader used by the runtime and the curator. |
| AC4 runtime config | `src/config.rs` | Schema v5 with optional absolute `scene_pools_file`; the main config, sidecar, and all referenced pool packages validate as one candidate off the Wayland owner; `reload-config` installs the validated registry+pool+rule state atomically without selecting. |
| AC5 selector | `src/scene_registry.rs`, `src/wayland_surface.rs` | One shared eligibility gate on every automatic selection path (startup/full selection, resume, explicit `reload`, `next`, CR-0006 same-family weather-variant); manual `scene <id>` remains an explicit override; no-candidate outcomes fail closed/retain with bounded reasons; no new selection triggers. |
| AC3 curator | `src/curator.rs`, `src/bin/curator.js`, `src/bin/curator.html` | Pool enable/disable, per-scene enable/time/weather/temperature editing, eligibility explanations under one preview snapshot, thumbnails, keyboard navigation, atomic same-user sidecar saves, and an explicit *Request config reload* action. |
| AC6 preview | `src/bin/curator.js` (WebGL2 port), `src/bin/curator_palette.js` (new twin), `src/animation.rs` (goldens test) | Authored palette cycles and known ToD variants preview from the exact GPU byte payloads with all large uploads outside the per-frame path; a fidelity chain ties the preview to the accepted CPU reference. |
| AC7 tests | `tools/tests/test_preview_palette_fidelity.py` (new), `fixtures/preview-palette-goldens.json` (new) | Atomic-save/permission/rollback tests, cross-language preview fidelity check, privacy-bounded diagnostics, and the live host records below. |

## AC4 — runtime config transaction

- `schema_version = 5` adds optional top-level `scene_pools_file`, required to
  be an absolute path; v1–v4 files stay readable and reject the key.
  Absence preserves legacy registry/selection behavior exactly.
- `AppConfig::prepare_sources` validates the main config, sidecar, and every
  referenced pool package as **one candidate** on the caller's worker thread.
  Registry pools must name configured-registry packages; library pools resolve
  through the adjacent library catalog and their packages fully validate and
  decode before the candidate may commit. Any error keeps the previous live
  config/pool set (no partial apply).
- A successful `reload-config` commits registry+pool+rule state in one step
  (`SelectionController::replace_sources`), invalidates queued adaptive
  candidates, never selects, never deletes assets, and starts no provider
  request. Test: `pool_candidate_commits_atomically_without_selecting_and_keeps_resident_scene`
  (resident scene/generation retained), plus the existing reload rollback tests.
- **Resolution decision flagged for maintainer review:** the approved config
  shape carries only `scene_pools_file`, so `library`-sourced pools resolve
  through `library.toml` next to the configured sidecar, falling back to the
  curator's XDG default library root. This is recorded in
  [scene-pools](../scene-pools.md); changing it needs a roadmap change.

## AC5 — the shared automatic-selection filter

- `src/scene_pools.rs` is the single eligibility implementation (pool enabled,
  scene enabled, every configured criterion matched from one snapshot;
  `weather_any` values OR, categories AND; a configured criterion with an
  unknown required value never matches). The runtime selector and the curator
  explanations both call it.
- Candidate composition follows the accepted AC1 policy: non-adaptive pool mode
  selects from the union of enabled pools (library packages join the candidate
  set); adaptive mode keeps the CR-0006 registry as the family/band source and
  uses pool state only as a filter; library entries stay manual-only there.
- CR-0006 interaction: band/family selection is computed first and unchanged;
  the family's precipitation variants are filtered in the approved
  same-family fallback order (snow/rain → cloudy → clear). A rejected candidate
  never crosses family/band, never falls back to V29, and never mutates stored
  family/band state (`resulting_state` is set only from accepted decisions and
  persisted only after commit).
- No-candidate behavior: startup exits nonzero before presentation with
  `startup selection unavailable: no eligible scene`; `next` is a no-op with
  reason `no eligible scene for next` (surfaced as `"outcome": "noop"` when a
  sidecar is configured; legacy no-sidecar responses unchanged); resume and
  weather-variant attempts retain the resident scene and log
  `trace adaptive-scene-selection state=no-eligible-candidate …`. The
  designated fallback is selectable only inside the eligible set.
- Trigger matrix unchanged: `reload-config`, clock/solar ticks, ordinary
  weather refreshes, resize, and hotplug never select (tests assert the
  ignored-trigger matrix with the gate active).
- Tests: `pool_gate_constrains_automatic_paths_and_manual_id_is_an_override`,
  `gated_next_cycles_eligible_entries_with_wrap_and_no_op_reasons`
  (deterministic order + wrap + skip-disabled + both no-candidate outcomes),
  `designated_fallback_is_selectable_only_inside_the_eligible_set`,
  `temperature_criteria_need_known_values_and_match_exact_boundaries`,
  `adaptive_variant_filter_walks_preference_order_without_crossing_families`,
  `adaptive_full_selection_fails_closed_without_crossing_family_or_using_v29`,
  `adaptive_mode_pool_packages_stay_manual_only`,
  `absent_sidecar_preserves_legacy_next_and_startup_outcomes`, plus the
  scene_pools boundary/unknown/disabled/malformed matrix and the AC4 config
  tests (`schema_v5_sidecar_reference_is_opt_in_absolute_and_version_gated`,
  `pool_candidate_validates_sidecar_packages_and_rules_as_one_unit`).

## AC3 — curator command and UI

`worldwallctl curator [--library DIR] [--rules FILE]` with the approved XDG
defaults now provides: searchable/scrollable scene list with lazy thumbnails
and keyboard navigation (ArrowUp/ArrowDown/Enter, focus outlines, ARIA listbox
semantics), animated selected-scene preview, ToD scrubber where variants exist,
pool enable/disable for complete world-plus-cycle packages, per-scene
enable/time/weather/temperature controls, and per-scene eligibility under one
editable preview snapshot with itemized reasons. Editing changes only the
sidecar; *Request config reload* is an explicit action and the editor never
starts, stops, or terminates the wallpaper process. Bounded limitation:
pool membership is established at sidecar creation (one pool per source
collection); membership set changes use the offline validator.

Live functional evidence (automated Chromium session against the served
token-scoped UI, recorded offline/mocked scope): scene selection and preview
load (`640×480 · license unreviewed · authored animation · time-of-day atlas 5
rows (civil v1)`), honest unmapped-cycles display
(`static (no authored animation) · cycles unmapped-source-native`), rule edit →
canonical atomic save, snapshot-driven eligibility flip with two mismatch
reasons, pool disable persistence with all five package files intact, keyboard
selection change, reload-request failure message with no wallpaper reachable,
and reopen persistence of pools and rules.

## AC6 — preview fidelity chain

Recorded chain (each link has its own check):

1. `fixtures/preview-palette-goldens.json` ↔ `Animation::apply_palette` and the
   exact `metadata_bytes`/`cycle_index_bytes` payloads — Rust test
   `preview_palette_goldens_match_cpu_reference_and_byte_payloads` (static,
   hue-cycle, brightness-pulse, palette-cycle, BlendShift cases at three
   times; regeneration command recorded in the fixture and
   [tooling](../../tools/README.md)).
2. `src/bin/curator_palette.js` (JS twin) ↔ the same goldens — Node.js check in
   `tools/tests/test_preview_palette_fidelity.py` (**exact match on the first
   recorded run**).
3. WebGL2 shader port ↔ JS twin — in-page readback self-check over a 256-color
   index strip at three probe times. Live result on the assigned host with the
   authored-animation derived package: **"Preview self-check: shader palette
   matches the reference twin exactly."**
4. `Animation::apply_palette` ↔ the GPU path — the existing accepted S04
   readback gate (`gpu_animated_palette_readback_matches_cpu_reference_vectors`
   and the ToD byte-floor readback), untouched here.

All large resources (indices, palette, metadata, cycle indices, ToD atlas/key
rows) are uploaded once per selected scene; per frame only clock/scrub uniforms
change. ToD scrubbing is injected preview state (civil v1 keyframes; solar v2
scrubs authored keyframe order) and never implies runtime time-triggered
selection. Note recorded for review: the accepted CPU reference applies hue
amplitude before the phase wrap while the shader wraps phase first; the two
agree on the golden set (integer amplitude / sub-unit phase) and diverge only
for fractional hue amplitudes at total phase ≥ 1 — pre-existing, unchanged by
this increment, and deliberately outside the golden set.

## AC7 — transaction, privacy, host, release validation

- Atomic save with same-user permissions: temporary file + `O_NOFOLLOW`,
  `fsync`, `rename`, directory `fsync`; rendered document must re-validate and
  round-trip canonically before any write; failures preserve the previous valid
  sidecar (unit test `editing_payloads_validate_and_save_atomically_with_private_permissions`).
  Live: created directory `0700`, sidecar `0600`; a deliberately invalid edit
  left the previous file byte-identical (unit test).
- Privacy: eligibility reasons name rule categories only; no coordinates,
  weather request details, or telemetry appear in UI text, responses, or logs
  (the sidecar stores user-authored rules; the curator fetches nothing).
- Host evidence on the assigned Hyprland setup (real compositor/GPU): native
  Wayland Firefox (isolated temporary profile, telemetry-disabled prefs) opened
  the curator with zero Wayland errors and rendered the complete UI; a `grim`
  capture of the session is retained
  (`~/.local/state/worldwall/s12-implementation-20260923/host-firefox-curator.png`)
  showing the library, preview, eligibility (including `its pool is disabled`
  for the disabled pool), and controls beside the untouched running wallpaper.
  Close lifecycle: closing the Firefox window via the compositor left no
  `worldwallctl`, Firefox, or server process behind and no error output — the
  command exited with its window.
- Corrupt/missing inputs: missing default rules file ⇒ no sidecar + create
  flow; explicit missing `--rules` ⇒ bounded error (existing rule, unit-tested);
  malformed sidecars/rules/catalogs rejected without partial state (scene_pools
  and AC4 tests).
- Not-run, recorded as such: synthetic keyboard/mouse input **on the host
  window** (no `wtype`/`ydotool` on the assigned host and the Lua dispatcher
  wrapper rejects `sendshortcut` syntax; input behavior was exercised in the
  automated browser session and keyboard navigation tests instead);
  *Request config reload* against the **live production wallpaper** (deliberately
  not sent to avoid disturbing the assigned session; the unreachable-path
  message and the underlying `reload-config` transaction are covered by tests);
  a live runtime pool-mode wallpaper run (offline tests only this increment);
  license review (unchanged: `unreviewed`, artwork unbundled/unpublished).

## Validation results (offline unless noted)

- `cargo fmt --all --check`
- `cargo clippy --offline --all-targets --all-features -- -D warnings`
- `cargo test --offline --all-targets --all-features` — 129 application tests
  and 50 `worldwallctl` tests passed; 3 Vulkan tests ignored as before
- `cargo build --offline --locked --all-targets --all-features`
- `python3 tools/check_docs.py` — 127 Markdown files, 13 stages, 7 skills
- `python3 -m unittest discover -s tools/tests -v` — 98 tests (2 new)
- `node --check` on `curator.js`, `curator_palette.js`; Node.js golden check
  passed (exact match)
- `git diff --check`
- Live host records as described under AC3/AC7 (browser-served UI and native
  Firefox on Hyprland; artifact directory
  `~/.local/state/worldwall/s12-implementation-20260923/`)

## Handoff

Unmet gates remain the maintainer's acceptance reviews for AC3–AC7 (including
the library-root resolution decision above and the host-input gap), the
deliberately unsent live `reload-config` request, a live pool-mode wallpaper
run, and license review before any distribution. Next safe actions: an
assigned-host runtime pool-mode smoke run under authorization, then a joint
AC3–AC7 review. CR-0006 supplemental acceptance and the historical S00–S11
records are untouched.
