# CR-0010 — Runtime adaptive-mode toggle in `worldwallctl`

- State: applied
- Proposer / date: Pi coding agent / 2026-10-01
- Decision owner: `simplysoup`
- Baseline version / affected IDs: approved contract v0.8 → v0.9; WW-06, WW-10; supplemental S10-AC10 and S12-AC8 (S12-AC1/AC4/AC5 source-policy context)
- Motivation / evidence: the user clarified that they want to toggle adaptive mode through `worldwallctl` after CanvasCycle/library scenes remained unavailable to `worldwallctl next`. The current approved S12 policy intentionally keeps library pool scenes manual-only while adaptive mode is active; in non-adaptive pool mode, enabled library pool packages join the automatic candidates. See [CR-0008](CR-0008-worldwallctl-scene-curator.md), [scene-pools source resolution](../scene-pools.md#runtime-consumption-s12-ac4ac5), and the [CLI/IPC contract](../ipc.md).

## Exact delta

| Item | Contract v0.8 behavior | Proposed after | Reason |
| --- | --- | --- | --- |
| WW-10 / CLI and IPC | Controls are `status`, `reload`, `reload-config`, `scene <id>`, `next`, and `weather`; curator is not an IPC command. Adaptive mode is selected only through config. | Add `worldwallctl adaptive status`, `worldwallctl adaptive on`, and `worldwallctl adaptive off`, backed by a bounded same-user IPC request. Report the effective runtime mode. The toggle is process-local: it does not edit the config file; process restart or a successful `reload-config` restores the configured value. | Permit an explicit operator-controlled mode switch without silently editing private configuration. |
| Adaptive candidate sources / CR-0008 | While adaptive mode is active, library pool packages remain manual-only; they are not CR-0006 families. | Turning adaptive mode off prepares the existing non-adaptive pool candidate set, where enabled and eligible library packages join registry scenes for existing automatic triggers, including `next`. Turning it on returns library packages to manual-only candidates and requires the existing Open-Meteo, explicit-location, and complete adaptive-registry validations. | Make curated CanvasCycle scenes available through the already-approved non-adaptive pool behavior without weakening adaptive-family validation. |
| Transition semantics / WW-06 | `reload-config` changes configuration without selecting. There is no CLI mode-change event. | `adaptive on/off` changes only effective mode and validated candidate sources; it never selects a scene, changes pool rules, location/provider configuration, or starts a provider request. A later explicit `next` or `reload` uses the new mode. Each transition is prepared off the Wayland owner and commits atomically; any validation/load failure retains the previous mode and sources. | Preserve the environment → selector → scene resources → renderer boundary, no-new-selection-on-config-change policy, and failure rollback. |
| Tests and acceptance | No protocol or runtime adaptive-toggle tests exist. | Add protocol/CLI tests and runtime tests for idempotent status/on/off, off-mode library candidates through the shared eligibility gate, on-mode manual-only library behavior, rejected enablement without prerequisites, failure rollback, unchanged resident scene/generation, no provider request, and unchanged selection triggers. Submit the additive S10/S12 evidence for maintainer review. | Cover the new public command, source transition, and failure paths without treating unit tests as live-host acceptance. |

### Proposed command semantics

- `worldwallctl adaptive status` reports the effective mode of the running process.
- `worldwallctl adaptive off` disables adaptive family/precipitation selection for this process. It does not disable an otherwise configured weather cache/provider. In pool mode, enabled library scenes with matching rules become automatic candidates, so `worldwallctl next` can reach them. A configured scene rule or disabled pool can still exclude a scene.
- `worldwallctl adaptive on` enables the current configured adaptive policy for this process only after validating that Open-Meteo, explicit location, and the complete adaptive registry are available. Library scenes remain manually selectable but are not adaptive-family candidates.
- Either successful toggle leaves the resident scene in place. Use an existing explicit selection command afterward if a scene change is desired. Toggling is not a new scene-selection trigger.
- The file is not rewritten. `reload-config` and process restart reapply the file's `adaptive_scenes` value. A persistent config-editor mode is intentionally not included in this proposal; it would need its own safe atomic file-update and conflict semantics.

## Alternatives and impact

- **Do nothing:** retain the current policy. With adaptive mode enabled, library scenes remain manual-only; they can still be selected explicitly with `worldwallctl scene <id>`.
- **Edit the config manually:** set `[weather].adaptive_scenes = false` and use `reload-config`. This already selects the approved non-adaptive pool behavior but does not satisfy the request for a `worldwallctl` toggle.
- **Proposed runtime-only IPC toggle:** add a mode operation to S10 and prepare/revalidate the complete registry-plus-pool candidate off the Wayland owner before an atomic mode/source commit. No config persistence is added. Existing triggers, privacy defaults, and artwork/license rules remain unchanged. Implementer estimates are not asserted; dependency or resource-budget changes are not expected, subject to code review.
- **Persistent CLI config editing:** could make the choice survive restart, but would make `worldwallctl` mutate the authoritative config file and require reviewed TOML-preserving writes, permission/symlink handling, race/conflict behavior, and rollback if runtime reload fails. This is excluded from the proposed first increment.

The toggle does not make library scenes adaptive families, infer day/night metadata, invent missing palettes, or alter the pool sidecar. It only selects between the already-defined adaptive and non-adaptive source policies. With no configured sidecar/library pool, turning adaptive off cannot expose CanvasCycle packages that are not loaded and assigned to an enabled pool. The existing S12 review state and AC5 correction review remain unchanged; no historical S10/S12 acceptance is reopened or silently replaced. No renderer, Wayland lifetime, output lifecycle, provider, or session-start behavior is changed by the request itself.

Rollback removes the new CLI/IPC command and returns adaptive-mode changes to config reload only. With the command disabled, the configured `adaptive_scenes` setting remains authoritative and the existing library-manual-only rule continues to apply while adaptive mode is active.

## Approval

- Decision / conditions: accepted as proposed: `adaptive status|on|off` is runtime-only; successful toggles only update validated effective mode/source state and never select or initiate a provider request; config reload or restart restores the file setting. Existing eligibility, manual-selection, privacy, and trigger semantics remain as stated above.
- Approver identity / date: `simplysoup`, 2026-10-01 (explicit user direction: “Accept and apply proposal CR-0010”; identity follows the recorded project instruction for unqualified maintainer approvals).

## Application

- Applying revision / evidence: implemented from baseline `eb9a876`; [CR-0010 implementation and validation record](../updates/2026-10-01-cr0010-adaptive-mode-toggle.md) identifies the exact source hashes and checks.
- Updated contract version / files: v0.9; `docs/delivery-contract.md`, `docs/ipc.md`, `docs/architecture.md`, `docs/scene-pools.md`, `docs/scene-format.md`, `docs/stages/10-ipc-config.md`, `docs/stages/12-scene-pool-curator.md`, `docs/roadmap.md`, `docs/status.md`, `README.md`, `docs/release.md`, `docs/knowledge/failure-modes.md`, [FM-0019](../knowledge/failures/FM-0019-config-reload-test-assumes-provider-start-is-synchronous.md), and this proposal/register; runtime/CLI changes in `src/ipc.rs`, `src/bin/worldwallctl.rs`, `src/config.rs`, and `src/wayland_surface.rs`.
- Accepted evidence made stale: none. Historical S10/S12 acceptances remain unchanged; supplemental S10-AC10 and S12-AC8 are open for maintainer review and assigned-session evidence.
- Linked roadblock disposition: none.
