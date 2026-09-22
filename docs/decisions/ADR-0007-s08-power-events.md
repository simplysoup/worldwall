# ADR-0007 — S08 power-event adapter and paused animation clock

- Status: accepted
- Owner / date: Pi coding agent / 2026-09-22
- Approval identity / date: `simplysoup` / 2026-09-22
- Affected stages / requirements: S08; WW-06, WW-08, WW-11
- Related change proposal: none; this stays within the approved suspend/resume scope

## Context and constraints

S08 must pause rendering on real sleep, resume only after valid output
configuration, coalesce duplicate power notifications, and keep a missing
power service visible without blocking Wayland. A wall-clock jump is not a
resume event. Network/weather work must remain outside Wayland dispatch, and
scene replacement must retain the previous valid resources on failure.

The target host exposes `systemd-logind` on the system D-Bus. The adapter API
must therefore be verified against the actual pinned system-bus API rather than
assuming a compositor-specific power hook. Real sleep-cycle acceptance remains
an operator-authorized live procedure and is not implied by this decision.

## Alternatives

1. Poll the wall clock or `/proc`: rejected; it cannot distinguish sleep from a
   time adjustment and creates unnecessary wakeups.
2. Invoke `loginctl` or `systemctl` as a child process: rejected; it adds a
   process/command dependency and does not provide a subscription lifetime.
3. Subscribe to `org.freedesktop.login1.Manager.PrepareForSleep` on the system
   bus and monitor `NameOwnerChanged`: chosen. The subscription is isolated on
   worker threads and enters the calloop loop through a bounded channel.

## Decision

`PowerEventSource` uses pinned `zbus` 5.14 blocking APIs only on worker
threads. It subscribes to:

- `org.freedesktop.login1.Manager.PrepareForSleep(bool)`, where `true` emits
  entering-sleep and `false` emits resumed; and
- `org.freedesktop.DBus.NameOwnerChanged` filtered to
  `org.freedesktop.login1`, so service loss/restoration is explicit.

Initial `NameHasOwner` state is reported as available or missing. System-bus,
subscription, receive, or worker-start failure emits a disconnected/degraded
power event; the application continues its normal rendering/manual-selection
path and never fabricates a resume. Events cross into the Wayland thread over a
bounded calloop channel. The state machine coalesces repeated sleep/resume
signals and assigns a generation to each pause/resume transition.

On entering sleep, the frame and weather timer sources are removed, callback
readiness is cleared, and no GPU resources are destroyed. On a coalesced
resume, the application waits for a nonzero configured physical extent, makes
one immutable environment snapshot, starts one bounded stale-weather attempt,
and invokes the existing explicit `Resume` selection transaction once. The
renderer resumes with a bootstrap frame; unchanged content uses the existing
content-key transaction and does not upload again.

The animation clock is monotonic and paused across sleep. It excludes the
suspended duration and remains in the existing bounded 1024-second phase
window. Civil time continues to be supplied separately to the optional
palette timeline and is not used as a frame deadline.

## Consequences and validation

This adds the pinned `zbus` dependency and its locked transitive graph. Tests
cover duplicate/coalesced events, stale generation invalidation, missing
service degradation, paused-clock duration exclusion, and scheduler pause/
resume behavior. A bounded real Wayland/Vulkan startup trace observed systemd-logind as
available and presented normally. Three operator-authorized suspend/wake
cycles, an offline-weather run, and a failed-candidate live resume were
recorded. Service disconnect was not run. The first topology helper attempt
used the legacy monitor mutation and was rejected by the installed Hyprland
0.56.2 non-legacy parser. The corrected helper then ran with operator
authorization: it disabled non-selected `HDMI-A-1` during the real sleep
signal, verified `disabled=True`, restored the captured 1920x1080@60/layout
rule after resume, verified `disabled=False`, and Worldwall resumed selected
`HDMI-A-2` with one scene upload. The corrected run also added retry handling
for the compositor's resume-time connector race. The live logs are recorded
in the S08 update.

The adapter, paused-clock policy, and S08 live evidence were reviewed by
`simplysoup` on 2026-09-22. This record does not claim energy savings or
universal D-Bus compatibility.

## Approval record

Accepted by `simplysoup` on 2026-09-22. See [S08 implementation evidence](../updates/2026-09-22-s08-suspend-resume.md).

## Supersession

None.
