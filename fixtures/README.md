# Fixture provenance

S00 uses no downloaded or bundled artwork. Project code and documentation are
MIT-licensed; generated Effect Games packages are local inspection fixtures
only and are not part of the tracked or published Worldwall delivery.

## Synthetic fixtures

The original S00/S03 fixture plan uses a deterministic tiny indexed scene with
indices `0`, `1`, `2`, and `255`, an opaque 256-entry RGBA8 palette, and text
vectors. Those outputs remain outside the repository.

## Effect Games reference inventory (CR-0003)

`effectgames-reference-manifest.toml` is the tracked metadata-only inventory
for the 12 catalog scene families and 22 catalog variants observed on
2026-09-22. It records stable family/variant IDs, catalog names, scripts,
weather labels, time windows, source-specific palette remaps, and the explicit
unavailable December source. It is not a license or an asset bundle.

The available 21 variants may be materialized locally after an authorized
source fetch with:

```sh
python3 tools/generate_effectgames_fixtures.py
python3 tools/generate_effectgames_fixtures.py --check-only
```

The generator consumes browser-captured responses under
`fixtures/untracked/effectgames/`, writes 640×480 indexed PNG packages,
source-derived authored-byte `blend_shift` animation fixtures where catalog
cycles are active. The generator follows the inspected site's rate / 280 /
range-length mapping with its speed-4 control, rather than treating source
rates as arbitrary full-cycle speeds. It also writes contained optional
civil-time atlases when the source provides a timeline, and an explicit local
`registry.toml`. Four catalog-listed sources
have no active source cycles and remain static; the generator does not invent
animation for them. It uses only the Python standard library. `--check-only` parses every catalog-listed response and
requires each generated package; it never treats the V12 missing LBM as a
pass.

Every generated package has `SOURCE.txt` with its source URL, dimensions,
stable IDs, and remap. The raw responses, palettes, indices, atlases, and
package directories remain untracked and ignored. Do not publish, commit, or
redistribute them without a separate source/license review. The source
catalog is <https://www.effectgames.com/demos/worlds/scenes.js>; a catalog
listing does not establish permission to copy its artwork.
