# CanvasCycle cycles mapped to authored animation (Desert Heat)

**Date:** 2026-09-23 · **Direction:** maintainer ("images without a time
component (from canvas cycle) are not being animated (specifically desert
heat). Reload those to ensure they work correctly.") · **Recorded by:** Pi
coding agent.

## What changed

The accepted V25HEAT capture kept its 16 source cycles as unmapped metadata
(AC2), so its package was static by construction. On maintainer direction the
importer now maps those cycles to animation using the Effect Games `Palette`
semantics already established and validated in
[FM-0013](../knowledge/failures/FM-0013-effectgames-cycle-rate.md) (same
engine and `reverse/rate/low/high` fields as the CR-0003 world scenes):

- `tools/import_canvascycle_capture.py` derives `animation.toml` with one
  authored-byte `blend_shift` group per active cycle:
  `speed = rate * source_speed_adjust / 280 / member_count`, source reverse
  mode `2` signed as reversed; other reverse modes are rejected, never mapped.
  `--source-speed-adjust` (default `1.0`, the site's base speed control; the
  CR-0003 fixtures used `4` for their requested fast run) is recorded in the
  manifest header with the formula.
- Materialized packages now declare `animation` (not `source_cycles`), matching
  the accepted catalog rule that mapped cycles are animation data and
  `source_cycles`/`cycle_semantics` remain for captures with unrepresented
  modes. The capture file remains the raw record.
- Importer tests updated and extended (exact speed values, signed reverse,
  mode-1 rejection, inactive-cycle handling): 12 tests pass.

## Regeneration and verification

- V25HEAT re-imported from the explicit local capture (sha256 matches the
  recorded provenance) into the scene library; the previous unmapped package is
  retained outside the library as `canvascycle-v25heat-unmapped-backup` under
  `~/.local/state/worldwall/s12-implementation-20260923/`.
- `tools/scene_pool_library.py --write-catalog` + `--check-only` pass:
  `animation_groups=16`, `source_cycles=0`, pools unchanged (2), rules
  unchanged (0).
- `worldwallctl curator` restarted against the rebuilt package; the preview
  reports **"640×480 · license unreviewed · authored animation"** (was "static
  (no authored animation)"). The maintainer's Firefox window was captured live
  rendering all 19 scenes with correct thumbnails and controls.

Open visual confirmations for the maintainer (my automated test browsers'
canvas captures are unreliable per FM-0018): motion in the preview (Play) and
the time-of-day slider's visible effect on registry scenes. The slider is
expected to do nothing on Desert Heat (no time-of-day atlas) and to shift
palette rows on the registry scenes.

Animation pace is re-derivable in one command (`--source-speed-adjust N`) if
the base control is not to taste; with the session's 0.4 global multiplier the
default yields about 0.7–1.4 full range rotations per second for these rates.

## License state

Unchanged: `unreviewed`, private, unbundled, unpublished. Publishing or
distributing artifacts remains separately unauthorized.
