# CR-0010 — Runtime adaptive-mode toggle

- **Date / implementer:** 2026-10-01 / Pi coding agent; proposal accepted by `simplysoup` on 2026-10-01.
- **Baseline:** contract v0.8 → v0.9; implementation based on `eb9a876a663e3906375df322ee3813f11f2ae3cc`.
- **Requirements / supplemental gates:** WW-06, WW-10; S10-AC10 and S12-AC8. Historical S10/S12 acceptances and the existing S12 review state are unchanged.
- **Kind:** Rust unit/integration tests and documentation. No assigned-session or live compositor test.

## Applied behavior

Added `worldwallctl adaptive status|on|off` to protocol v1 as a bounded same-user IPC command. Status reports effective mode. On/off are idempotent and process-local: they do not edit config, select a scene, change pool rules or provider/location settings, or issue weather requests. Enabling validates the configured Open-Meteo provider, explicit location/timezone, and full adaptive registry. The source candidate (registry, sidecar, and library pool packages) is revalidated on a bounded worker and committed on the event-loop owner. Adaptive-off pool mode includes eligible library packages in the existing shared candidate filter; adaptive-on keeps them known/manual-only and does not promote them to adaptive families. Failed preparation retains the old config/mode/sources. `reload-config` and restart restore the configured `adaptive_scenes` value.

The contract is now v0.9. Public semantics and the separate S10-AC10/S12-AC8 supplemental review gates are in the linked [CR-0010 record](../changes/CR-0010-worldwallctl-adaptive-toggle.md), [IPC reference](../ipc.md), [S10](../stages/10-ipc-config.md), and [S12](../stages/12-scene-pool-curator.md).

## Validation

| Check | Result |
| --- | --- |
| `cargo fmt --all --check` | Pass |
| `cargo clippy --offline --all-targets --all-features -- -D warnings` | Pass |
| `cargo test --offline --all-targets --all-features --no-fail-fast` | Pass: 139 main tests, 81 CLI tests, 3 example tests; 3 Vulkan tests ignored |
| `cargo build --offline --locked --all-targets --all-features` | Pass |
| `python3 tools/check_docs.py` | Pass: 288 Markdown files, 13 stages, 7 skills; archive intact |
| `python3 -m unittest discover -s tools/tests -v` | Pass: 117 tests |
| `git diff --check` | Pass |

Final SHA-256 fingerprints for the four implementation files:

| File | SHA-256 |
| --- | --- |
| `src/bin/worldwallctl.rs` | `72a9c77ab3af27ed3f9f6d1a3a2be047e8c295b8df1a9fdb5967e9ebfe1de5e7` |
| `src/config.rs` | `80cd09002122839ffcb24cd22b3d8e84d17f5570e03e3e71da2cc1f26aa6bc4e` |
| `src/ipc.rs` | `91b26437f5d80e625af21f346daf48f12ab40eb7132e7105767f0bdc44706420` |
| `src/wayland_surface.rs` | `265191a9ee498a3e40896c860b1860b70caa202a9d3844a50a9efb19f0beb74d` |

New coverage includes framed protocol parsing/encoding for all three operations and invalid mode rejection; CLI form mapping; idempotent action classification; enabling validation for explicit location/timezone and a complete five-band adaptive registry; actual schema-v5 source recomposition in both directions; and runtime on/off commits that retain the resident scene and selection generation, make library packages available only through the non-adaptive shared pool gate, and make no provider request. Source prep remains separate from the Wayland owner.

One full-suite run exposed an existing test race: the fake replacement weather provider had not entered async `fetch` by the immediate `new_started` assertion. The test now waits for that observable state with a bounded deadline; [FM-0019](../knowledge/failures/FM-0019-config-reload-test-assumes-provider-start-is-synchronous.md) records the failure and guardrail. The final full all-target run passed. The first pre-change baseline attempt also had one `NotFound` failure reading the tracked preview palette golden; a subsequent full all-target run passed without changing that fixture. The cause of the isolated baseline failure was not established.

## Not run / limitations

No IPC command was sent to the installed session, no config/session/process was changed, and no weather request was made. Assigned-session end-to-end command verification and maintainer review of S10-AC10/S12-AC8 remain open; this update is not stage acceptance or a live-host claim. No art, license, package, or source asset was changed.

## Next action

`simplysoup` reviews the supplemental S10-AC10 and S12-AC8 evidence and assigns an authorized live IPC check when safe. Keep the installed session untouched until that separate authorization; existing S12 AC5 correction review remains open.
