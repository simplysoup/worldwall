# Full CanvasCycle import, ToD loop fix, and web-server curator

**Date:** 2026-09-23 · **Direction:** maintainer ("Looks perfect import the
remaining scenes from <https://www.effectgames.com/demos/canvascycle/>. Can we
have curator just open in a standard webserver and provide a link...") ·
**Recorded by:** Pi coding agent. Fetch of that listing and its captures is
explicitly maintainer-authorized; everything stays private/unpublished.

## Full CanvasCycle import — 35/35 scenes

- Fetched the source listing (`canvascycle/scenes.js`; exact `name`/`title`
  pairs; commented-out entries excluded) and each `images/<NAME>.LBM.json`
  capture to explicit local files, then ran the bounded AC2 importer per scene
  with explicit provenance (hash of each capture recorded; titles used
  verbatim as `<title> (<NAME>)`).
- Two parser corrections were required by the real capture data, both faithful
  to the engine semantics established in FM-0013 (and both tested):
  1. **Inert cycle slots** (`low == high`, e.g. the engine's zeroed unused
     slots) are accepted and skipped exactly like zero-rate cycles — they were
     previously rejected, blocking 18 scenes.
  2. The **overlap rejection** now considers only active real ranges
     (`rate != 0 && low < high`) — inert slots no longer falsely conflict,
     unblocking the remaining scenes.
- Result: **35 available, 0 unavailable**; catalog rebuilt and validated:
  `35 scene(s)`, `175 files`, `417 animation groups`, `source_cycles = 0`
  (all mapped). Every package declares authored `blend_shift` animation from
  its capture's cycles with `--source-speed-adjust 1` (site base; re-derivable
  per scene).

## Time-of-day slider fix (preview render bug)

The preview shader's per-fragment civil keyframe loop (256 iterations) is the
suspected browser GPU-process killer behind the crash reports and dead sliders
(FM-0018). The keyframe search now runs **once per frame in the client** and
the shader only performs the byte-floor fade between the two resolved atlas
rows — the same injected-state contract the runtime uses for resolved samples.
The slider is the preview's time input and now also drives the eligibility
snapshot (time class mapped from the slider; solar-derived classes remain
preview-ineligible).

## Layout changes (maintainer direction)

- Center column keeps the preview stack and gains the **Preview snapshot**
  (weather + temperature only; time comes from the slider).
- Right column: **Scene rule** (time of day / weather / min–max °C) on top,
  **Pools** with the save/reload actions in the middle, **Eligibility** at the
  bottom.

## Curator is now a plain web server

`worldwallctl curator [--library DIR] [--rules FILE]` no longer launches
Firefox (removed with its temporary-profile machinery at maintainer request);
it prints a token-scoped loopback link for the operator's regular browser and
serves until interrupted. The page behavior is otherwise unchanged.

## Verification

- Importer tests: 12 pass (including new inert-slot mapping coverage).
- Rust: 129 + 77 tests pass; Clippy `-D warnings` clean; locked release build.
- Catalog/sidecar validation passes (`35 scenes`, `2 pools`, `0 rules`).
- The served link loads the 35-scene library with per-scene animation.

Not claimed: license review (all `unreviewed`, unpublished), and no runtime
selection behavior changed (library packages stay manual/pool material; the
wallpaper's pool consumption is as previously accepted).

## Reproduction

```sh
python3 tools/import_canvascycle_capture.py --input <capture>.LBM.json \
  --output <library>/<id> --id <id> --source-url <capture URL> \
  --source-page https://www.effectgames.com/demos/canvascycle/ \
  --source-title "<title> (<NAME>)"
python3 tools/scene_pool_library.py --library <library> --write-catalog
```
