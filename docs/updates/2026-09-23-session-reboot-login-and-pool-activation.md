# Session reboot/login test pass and schema-v5 pool-mode activation

**Date:** 2026-09-23 · **Authorization:** maintainer ("Reboot test worked
perfectly. Update the config") · **Recorded by:** Pi coding agent ·
**Scope:** assigned Hyprland session deployment (`soupydesktop`), post-S12
acceptance.

## Reboot / next-login test — passed (maintainer observation)

The maintainer reports the authorized reboot test **worked perfectly**: the
hyprland startup hook's installed duplicate-safe `worldwall-session` launcher
came up at login with the configured time/weather-aware animation on both
outputs. This closes the long-standing `not-run` item for reboot/next-login
activation in the S11 deployment and CR-0006 supplemental evidence lists.
Evidence class: **maintainer live observation** on the assigned host; no
measurement artifacts were collected for it and none are claimed. Physical
suspend/resume of this deployment remains separately not-run.

## Schema-v5 pool-mode activation (authorized config change)

The maintainer authorized updating the session config after the reboot test.
Applied in a fail-safe order with backups, on the running session:

1. **Real sidecar created** at `~/.config/worldwall/scene-pools.toml` (the
   default the curator edits and the path the runtime references): an enabled
   `registry` pool covering all 18 adaptive-registry scenes and an enabled
   `library` pool with the imported capture `canvascycle-v25heat`. Validated
   with `tools/scene_pool_library.py --sidecar … --check-only` (2 pools,
   0 rules) before anything consumed it. Starting with every scene eligible
   preserves current selection behavior while curation adds constraints.
2. **Daemon binary updated** to the accepted build
   (`~/.local/share/worldwall/bin/worldwall`; previous binary backed up as
   `worldwall-daemon-replaced-20260923`). Required because the previous daemon
   rejects schema v5; updating config alone would have failed the next login.
3. **Config updated** to `schema_version = 5` with
   `scene_pools_file = "$HOME/.config/worldwall/scene-pools.toml"`;
   all other settings (fps, animation multiplier, registry, explicit
   location/timezone, Open-Meteo opt-in, adaptive scenes) preserved unchanged
   (v4 file backed up as `worldwall.toml-v4-backup-20260923`).
4. **Clean restart** through the user's own `worldwall-session` singleton
   launcher (previous instance stopped with SIGTERM first; the launcher's
   duplicate-safe lock was released). Startup performed exactly one selection
   (`v29-seascape`, generation 1) on both outputs (71, 73), zero degraded
   outputs, weather cache fresh per the existing provider configuration.
5. **Live one-candidate verification**: `worldwallctl reload-config` returned
   `{"applied": true, "reselected": false}` with the scene and selection
   generation unchanged — the schema-v5 config, sidecar, and pool packages
   validated and committed atomically without selecting (S12-AC4 contract) on
   the production session.

Backups and logs live under
`~/.local/state/worldwall/s12-implementation-20260923/`. Rollback: restore the
backed-up `worldwall.toml` (v4) and daemon binary, then restart via
`worldwall-session`; or simply remove `scene_pools_file` to return to legacy
registry selection on the accepted binary.

## Curator session state at handoff

- `worldwallctl curator` (default capture library, real sidecar) — the daily
  curation window; edits affect the live pool rules on the next
  `reload-config`.
- `worldwallctl curator --library ~/.local/share/worldwall/scene-library-demo
  --rules ~/.local/state/worldwall/s12-implementation-20260923/demo-scene-pools.toml`
  — an isolated demo window (own sidecar) over three private derived packages
  showing authored animation, ToD scrubbing, and multi-scene eligibility. The
  demo library's derived rows carry honest `effectgames-world-scene-derivation-v1`
  provenance and are accepted by the runtime loader only; the AC2 Python
  validator's fixed `source_format` mapping covers capture imports, so
  `--write-catalog`/`--check-only` does not accept this demo library (kept out
  of the default library for that reason). Extending the validator mapping
  would be a small roadmap change.

## Why the first demo looked wrong (see FM-0017)

- *Upside down*: preview shader Y-convention mix-up, fixed; the self-check now
  verifies spatial mapping (768 texels × 3 probes, exact match recorded).
- *No animation*: correct-by-policy for the imported capture — V25HEAT's
  source-native cycles are `unmapped-source-native` and are never synthesized
  into animation. Animated previews need packages with authored animation
  manifests (the demo library's derived packages).
- *Single scene*: only one CanvasCycle capture is locally available (AC2
  record); the demo library adds derived packages for presentation.

## Not claimed

This record covers deployment activation and reported reboot evidence only. It
adds no acceptance, no license state change (all artwork remains `unreviewed`,
unpublished), and no claim about live **constraining** adaptive pool filtering
(rules currently constrain nothing; every scene is eligible). Publishing or
distributing artifacts remains separately unauthorized.
