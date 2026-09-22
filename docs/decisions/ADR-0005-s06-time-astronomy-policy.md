# ADR-0005 — S06 civil time, astronomy, and seasons policy

- Status: accepted
- Owner / date: Pi coding agent / 2026-09-22
- Approval identity / date: `simplysoup` / 2026-09-22
- Affected stages / requirements: S06; WW-06, WW-08
- Related change proposal: none; this is a stage-local policy proposal and does not change the approved baseline

## Context and constraints

S06 must produce an immutable environment snapshot for the existing S05 selector
without coupling local civil time to the renderer's monotonic animation clock.
CR-0002 additionally permits an explicit local civil-seconds value to feed an
already selected scene's optional palette timeline; it does not make the
renderer a selector or weather consumer.
The snapshot must be deterministic in tests, must not infer location, and must
make missing or physically unavailable astronomical events visible. The current
repository has no timezone or astronomy database dependency, and the accepted
S05 selector already represents time, season, weather, and unknown values.

Location and timezone are privacy-sensitive configuration. A provider may read
only explicitly supplied configuration or injected test values. It must not use
IP geolocation, desktop location services, network requests, or telemetry.

## Alternatives

1. Use the host's timezone database and implicit desktop location: rejected for
   the first policy because it makes behavior and privacy depend on ambient
   machine state and provides no explicit location-consent boundary.
2. Use UTC and a fixed equatorial fallback: rejected because it is not a local,
   location-aware classification and would hide missing configuration.
3. Use explicit location plus an injected UTC-offset schedule: proposed. It is
   dependency-light and deterministic while still representing DST transitions;
   a later decision may add a named-zone resolver without changing snapshot or
   selector ownership.

## Decision

This decision defines the following v1 policy.

### Inputs and ownership

- `Location` is explicit latitude/longitude in decimal degrees. Latitude must
  be in `[-90, 90]` and longitude in `[-180, 180]`; both values are required
  together. Invalid or partial location produces an explicit unknown snapshot.
- `UtcOffsetSchedule` contains an initial offset in whole minutes and strictly
  ordered UTC transition instants with their subsequent offsets. Offsets are
  bounded to the civil range `[-14:00, +14:00]`. This schedule is the timezone
  source for v1 and can represent fixed zones and DST changes without ambient
  timezone lookup. The schedule is supplied by explicit configuration; no
  location or timezone is inferred.
- A wall-clock provider supplies UTC Unix seconds. Wall-clock values are used
  only to form civil snapshots. Frame deadlines and animation continue to use
  the existing monotonic scheduler and never use local-time arithmetic.
- The environment provider returns a value snapshot and, when explicitly
  configured, a bounded local civil-seconds value for the resident renderer
  resource. It performs no selection, scene loading, renderer work, or Wayland
  dispatch. A civil-time tick changes only that uniform input; it does not
  increment selection generation.

### Civil time and astronomy

- Local civil time is UTC Unix seconds plus the offset selected by the schedule
  at that UTC instant. Calendar conversion is integer-based and includes leap
  years and dates before the Unix epoch.
- Solar events use the location and local civil date. The default astronomy
  implementation uses a bounded sunrise/sunset calculation with the standard
  refraction-aware sunrise zenith. The provider interface remains injectable so
  boundary tests do not depend on floating-point event drift.
- For a normal day, dawn is the half-open 30-minute interval before sunrise,
  day is from sunrise through sunset, dusk is the half-open 30-minute interval
  after sunset, and the remaining time is night. Exact sunrise and sunset are
  day boundaries. If the calculated day has no sunrise, the snapshot reports
  `PolarNight`; if it has no sunset, it reports `PolarDay`. Provider failure or
  missing inputs reports `Unknown` rather than inventing an event.
- The existing selector-facing `TimeOfDay` gains explicit `PolarDay` and
  `PolarNight` values. Scene manifests continue to accept only the authored
  day/dawn/dusk/night/unknown tags; polar states therefore use the validated
  offline fallback unless a later schema decision adds polar tags.

### Seasons and unknowns

- Seasons use local civil month/day and the hemisphere: northern meteorological
  seasons begin on March 1, June 1, September 1, and December 1; southern
  seasons swap spring/autumn and summer/winter.
- Locations within one degree of the equator report `Unknown` season rather
  than claiming a hemisphere convention. Missing/invalid location or civil
  input reports `Unknown`. This is a policy boundary, not a weather result.
- Ordinary diagnostics identify only degraded/unknown state and provider
  status. They do not print precise coordinates or timezone transition data.

### Refresh semantics

A clock tick, midnight, sunrise/sunset crossing, resize, output hotplug, or
weather completion may update an available status snapshot and the resident
palette uniform, but never invokes
the S05 selector. Startup and explicit `reload`, `scene`, `next`, or resume
requests obtain one immutable snapshot and pass it to selection. A later S08
decision owns suspend/resume coalescing.

## Consequences and validation

The implementation can remain dependency-free and can be tested with fixed UTC
seconds, explicit offset transitions, injected solar events, leap-day vectors,
hemisphere and equatorial vectors, polar states, missing inputs, and invalid
coordinates. DST tests verify that the offset schedule changes local civil time
at the declared UTC instant without changing monotonic scheduler behavior.
Selection tests use the existing S05 trigger controller to prove that a changed
snapshot is inert until an explicit trigger.

This ADR is accepted by `simplysoup` on 2026-09-22 and amended by approved
CR-0002 on 2026-09-22. S06 acceptance is recorded separately in the stage
evidence after review of the policy, implementation, and validation results. A
later approved timezone or astronomy dependency must
preserve explicit inputs, unknown/polar states, and the environment-to-selector
boundary.

## Approval record

`simplysoup` approved ADR-0005 on 2026-09-22. This approval covers the explicit
location and UTC-offset schedule inputs, civil-time boundaries, polar and
unknown states, hemispheric season policy, and refresh ownership. S06 stage
acceptance is recorded separately in the S06 evidence.

## Supersession

None.
