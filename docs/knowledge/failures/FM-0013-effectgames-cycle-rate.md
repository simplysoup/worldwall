# FM-0013 — Effect Games cycle rate mapped as full-cycle speed

- Classification: reproduced
- Owner / date / affected versions: Pi coding agent / 2026-09-22 / CR-0003 local fixture generator before the source-rate correction
- Stage / requirements: S04-AC1, S04-AC3, S05-AC5, WW-03, WW-04
- Related roadblock / evidence: [CR-0003 evidence](../../evidence/2026-09-22-cr003-scene-variants.md); local live trace `/home/soup/.local/state/worldwall/cr003-effectgames-20260922/live-js-mapped-v05-rain.log`

## Trigger, symptom, and mechanism

The initial CR-0003 fixture generator treated an Effect Games `cycles[].rate`
value as an arbitrary full-palette cycles-per-second value and emitted a
linear-RGB `palette_cycle`. At the inspected rate `4914`, this produced values
such as `0.4914` or `15.0` depending on an invented divisor. The live result
looked unlike the reference and could show only a few temporal samples at high
speed.

The confirmed cause is a source-semantics mismatch. The inspected site creates
`Palette` cycles and computes `cycleRate = rate / floor(CYCLE_SPEED /
speedAdjust)`, with `CYCLE_SPEED = 280`; the range length then determines the
number of complete range rotations per second. Its default UI also enables
`blendShift`, which performs integer rotation plus authored-byte interpolation.
The relevant source is `palette.js` and `main.js` under
<https://www.effectgames.com/demos/worlds/>.

## Detection and reproduction

- Source evidence: `Palette.cycle` in the inspected `palette.js` and the
  `setSpeed(0.25/0.5/1/2/4)` controls in `main.js`.
- Local reproduction: generated `animation.toml` files with the old divisor,
  then ran the CR-0003 live package sequence at 30 FPS. A five-color range at
  the old `0.4914` mapping did not represent the source's range-rate behavior;
  the 15.0 mapping yielded only two rendered samples per complete cycle at 30
  FPS.
- The behavior was reproduced on the local Hyprland/Vulkan session, not on an
  assigned acceptance host.

## Recovery and prevention

The generator now maps active source cycles as follows:

```text
worldwall full-cycle speed = rate * source_speed_adjust
                           / 280
                           / (high - low + 1)
```

It uses the site's speed-adjust value `4` for the requested fast fixture,
selects authored-byte `blend_shift` for the site's default Blend mode, and
maps the observed active reverse modes `0` and `2` to forward/reverse signed
speeds. Unrepresented source ping-pong/sine modes must not be silently mapped;
add an explicit effect or record them as unsupported if such a source is
introduced.

Do not fix this by uploading a new full palette or pixel texture every frame.
The useful pattern remains an immutable index texture plus immutable palette /
cycle metadata and a small animation uniform update. The visual result must be
validated against the source's cycle semantics and the actual compositor frame
cadence.

## Regression coverage

The generator and `tools/tests/test_canvascycle_import.py` lock the formula,
the signed reverse mapping, and the rejection of unrepresented modes.

## Application note (2026-09-23, maintainer-directed)

On maintainer direction, the same mapping now also converts CanvasCycle LBM
capture cycles (same `Palette` engine and `reverse/rate/low/high` fields) into
`blend_shift` groups in `tools/import_canvascycle_capture.py`; V25HEAT's 16
cycles are derived this way for the scene library. The importer defaults
`source_speed_adjust` to the site's base control `1.0` (the CR-0003 fixtures
used `4` for their requested fast run) and rejects reverse modes other than
`0`/`2`. The capture file stays the raw record.

`python3 tools/generate_effectgames_fixtures.py --check-only` passes for all 21
available packages; generated representative mappings include `4914 / 280 /
5 * 4 = 14.04` cycles per second. The local V05 rain package then completed a
3-second 30-FPS Wayland/Vulkan run with one scene upload and clean owner-safe
teardown. This is implementation evidence, not assigned-host acceptance or a
perceptual equivalence claim. Add a pure source-rate conversion vector test if
the generator becomes a maintained production tool.
