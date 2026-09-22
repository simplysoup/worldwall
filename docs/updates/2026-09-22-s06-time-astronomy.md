# S06 civil time and astronomy implementation increment

**Date:** 2026-09-22
**Stage / requirements:** S06; WW-06, WW-08
**Owner:** Pi coding agent
**State:** in-progress; policy proposed and S06 is not accepted
**Decision:** [ADR-0005 — S06 civil time, astronomy, and seasons policy](../decisions/ADR-0005-s06-time-astronomy-policy.md)

## Bounded outcome

Start S06 with a dependency-free, injectable environment snapshot provider. The
slice keeps environment interpretation separate from the S05 selector and the
accepted renderer/pacing path. It provides:

- explicit finite latitude/longitude validation and an explicit UTC-offset
  transition schedule that can represent fixed offsets and DST changes;
- integer UTC-to-local civil calendar conversion with leap-day and pre-epoch
  coverage;
- injectable sunrise/sunset events plus a bounded NOAA-style provider, with
  30-minute dawn/dusk boundaries and explicit polar-day, polar-night, and
  unknown states;
- northern/southern meteorological season classification with an explicit
  equatorial unknown band; and
- tests proving changed snapshots are inert until an explicit S05 selection
  trigger.

No IP geolocation, network request, telemetry, precise-coordinate logging,
renderer input, Wayland callback, clock-tick reselection, or runtime timezone
lookup was added. The existing GPU animation clock remains monotonic and
independent of this module.

## Changed files

- `src/environment.rs` — civil calendar, explicit location/offset inputs,
  injectable wall-clock and astronomy interfaces, NOAA-style events, snapshot
  classification, and boundary/failure tests.
- `src/main.rs` — registers the environment module.
- `src/scene_registry.rs` — re-exports environment-owned snapshot enums and adds
  an explicit-trigger regression test; existing scene tag parsing remains
  limited to the accepted v1 authored tags.
- `docs/decisions/ADR-0005-s06-time-astronomy-policy.md` — proposed policy for
  boundaries, timezone source, polar/unknown behavior, seasons, and refresh
  ownership.
- `docs/decisions/README.md`, `docs/roadmap.md`, `README.md`, `docs/status.md` —
  record the proposed decision and authorized `Pi coding agent` S06 ownership;
  roadmap state is `in-progress`.

Source artifact hashes at this increment:

- `src/environment.rs` — `1bea37bc649a54c814bba115d8994474f5c444b534a22cbcf6736fe2a08cc2fc`
- `src/scene_registry.rs` — `598fe4b3bbf1dca27e66dea3366c9d46f086320652d557e2595b09dd696504f4`
- `docs/decisions/ADR-0005-s06-time-astronomy-policy.md` — `5cccd897d9ccca6a2178feee53f2f158e609e3c3c6dc24d311c1c7c569a1f54a`

## Validation

| Check | Result |
| --- | --- |
| `cargo fmt --check` | pass |
| `cargo test --locked --all-targets --all-features` | pass — 49 passed, 2 ignored GPU tests |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | pass |
| `python3 tools/check_docs.py` | pass — 72 Markdown files, 12 stages, 7 skills |
| `python3 -m unittest discover -s tools/tests -v` | pass — 33 tests |

Pure tests cover civil epoch/leap conversion, explicit DST offset transitions,
invalid/non-finite coordinates, twilight and exact boundary behavior, polar
states, northern/southern/equatorial season policy, missing-input unknowns,
NOAA mid-latitude events, and snapshot changes that do not select without an
explicit trigger.

## Gate status and limitations

- **S06-AC1:** partial; proposed ADR and pure boundary vectors exist, but
  maintainer approval and the full recorded fixture/evidence matrix remain
  outstanding.
- **S06-AC2:** partial; the environment module uses integer wall-clock values
  and does not touch `FrameScheduler`, while existing scheduler tests remain
  green. A broader integration record is still required.
- **S06-AC3:** partial; pure selector tests prove a changed snapshot is inert
  on `ClockTick` and applied on `Reload`. Runtime explicit refresh wiring and
  fixture-scene evidence are not implemented in this increment.
- **S06-AC4:** partial; location is explicit and no diagnostics expose it, but
  runtime config/default diagnostics are not yet wired.

ADR-0005 is proposed pending maintainer review. This increment does not move
S06 to review or claim M3, and S07 must not consume the policy as accepted until
that review and the remaining S06 gates are recorded.

## Next safe action

Obtain maintainer review of ADR-0005, then add explicit runtime configuration
and a refresh seam that supplies one immutable snapshot to the existing S05
startup/explicit-selection path without allowing clock or astronomy boundaries
to reselect scenes.
