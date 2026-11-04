# Changelog

All notable changes to Worldwall will be documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added
- Public-facing top README, replacing the status-heavy development log as the
  entry point. Development history remains in `docs/status.md` and
  `docs/roadmap.md`.
- GitHub Actions CI: `cargo fmt --check`, Clippy with warnings denied,
  the full locked test suite, and documentation checks
  (`tools/check_docs.py` plus the Python tool tests).
- This changelog.
- `tools/run_ci_tool_tests.py`, a CI tool-test runner that excludes the
  host-only theme-watch test (it needs APCA helper sources that live
  outside this repository, under
  `~/.local/share/worldwall-theme-sync/apca-helper`, and so cannot run
  in a fresh checkout). It is not a test removal: the test itself is
  unchanged and still runs on a host that has the helper.

### Changed
- Local home paths in documentation and evidence logs generalized to `$HOME`,
  so no local username appears in public-facing docs.

### Fixed
- Nothing user-facing in this cleanup pass; it is documentation and
  release-preparation work only.

## [0.1.0] — in development, not yet released

Worldwall 0.1.0 as described by `Cargo.toml` (Rust 1.94.0, MIT licence).
Features documented in `docs/architecture.md`, `docs/scene-format.md`, and
`docs/ipc.md`:

- Wayland layer-shell wallpaper engine for Hyprland, rendering with wgpu
  (Vulkan) on the background layer.
- Indexed pixel-art scenes with palette animation, including hue/palette
  cycling and blend-shift effects, and a global animation-speed multiplier.
- Time-of-day and solar-event palettes from explicitly configured
  location/timezone; no location or timezone is inferred.
- Optional weather-adaptive scene selection via Open-Meteo, disabled by
  default, with same-family precipitation variants only — weather never
  switches scene families.
- Scene pools and a read-only curator UI slice (`worldwallctl curator`),
  with an explicit sidecar model for pool rules.
- `worldwallctl` control over a running instance via a same-user Unix
  socket: status, scene listing/selection, reload, weather, and
  adaptive-mode status/on/off.
- Versioned configuration (schemas v1–v5), atomic `reload-config`, and
  suspend/resume handling via systemd-logind where available.
- A privacy-safe example config (`examples/worldwall.toml`) with no
  location, artwork, or registry bundled. Art assets are intentionally
  not bundled in this repository.

Known limitations, as recorded in `docs/status.md` and `docs/roadmap.md`:
physical resume and some supplemental acceptance gates remain not-run,
and the demo recording has not yet been captured on a live Hyprland host.
See those documents for the current, evidence-based status rather than
treating this changelog as an acceptance record.

## AUR publishing decision

**Decision: do not publish to the AUR yet.** Rationale, recorded 2026-11-04:

1. The repository is still private, so a public AUR source fetch is
   unverified — `docs/release.md` already notes an anonymous fetch was
   denied pending GitHub authentication.
2. The fresh-clone build test and CI run have not yet completed, so a
   published recipe would point at an unverified build.
3. The author-identity history decision is still open; publishing now
   would pin commit SHAs that a rewrite would change.
4. The demo has not been captured on a live host.

Publish only after: the repo is public, CI is green on main, a public
source fetch of the pinned revision succeeds, and the demo is real.
Until then, build from the source checkout or transfer a locally built
package archive, as described in `docs/release.md`.
