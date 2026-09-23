# CR-0006 — Weather-adaptive scene families and precipitation variants

- State: applied to contract v0.6; implementation and acceptance gates remain open
- Proposer / approval date: `simplysoup` / 2026-09-23
- Decision owner: `simplysoup`
- Baseline before change: contract v0.5
- Affected requirements/gates: WW-06/07/08/10; S05-AC3/4 and CR-0006 AC6/7; S07-AC1/4/7; S08-AC2/3/6; S10-AC3/7/8
- Motivation/evidence: user direction in the 2026-09-23 session; [CR-0005](CR-0005-configured-location-weather-solar-lighting.md), [scene catalog](../../fixtures/effectgames-reference-manifest.toml), [scene format](../scene-format.md), and [S07 provider policy](../decisions/ADR-0006-s07-open-meteo.md)

## Applied policy

| Item | Contract v0.5 | Contract v0.6 | Rationale |
| --- | --- | --- | --- |
| WW-06 / S05-AC3 | Weather timer/completion changes cache only and never changes the scene. | With valid explicit schema-v3 opt-in, a fresh known weather result may replace only the precipitation variant in the active family. It cannot switch families. Full family selection remains limited to startup, coalesced resume, and explicit full-selection commands. | Preserve family identity between full-selection events while reflecting precipitation. |
| WW-07 / S07-AC1/4 | Optional weather is default-off and cache-only with respect to scene state. | Keep the bounded approximately 15-minute poll and default-off provider. With opt-in, accepted fresh weather may request a same-family variant; stale/unknown/error states retain the resident scene. | No faster cadence or automatic network consent. |
| Scene metadata | No explicit family or temperature metadata. | Scene manifest v2 may specify `family` (family ID) and `temperature_band`; adaptive registries must validate all variants, all five bands, per-family clear fallback, and the V29 clear offline fallback. | Make same-family behavior testable without parsing IDs. |
| Config compatibility | Schema v1/v2 preserve existing behavior. | Schema v3 adds `[weather].adaptive_scenes = false`; `true` requires Open-Meteo, explicit location, and a valid adaptive registry. | Preserve CR-0005 defaults and avoid changing existing users' behavior. |
| WW-08 / startup | One selection uses the available environment snapshot; late weather cannot reselect. | Adaptive startup/resume/full selection uses available temperature and persisted family/band state; first start without usable data uses clear V29. Later fresh weather may change only that family's variant. | Permit an offline-first weather-aware boot without a second family selection. |

## Normative selection rules

**Weather poll/completion:**

1. Only a successful fresh result with a recognized precipitation appearance can request a candidate. Disabled, stale, failed, unknown, or missing data retains the current scene.
2. Normalize provider appearance as snow → snowy; rain or precipitating storm → rainy; cloudy/fog → cloudy; clear → clear. Do not infer precipitation from temperature.
3. Search only the active family. Prefer the exact variant; otherwise use snow → cloudy → clear, rain → cloudy → clear, and cloudy → clear. No cross-family fallback is permitted.
4. Decode adaptive candidates off the Wayland event thread and pass them through the existing transactional renderer-swap boundary. Failed, stale, or superseded work retains the last valid scene. Weather temperature crossings never change family.

**Startup, resume, and explicit full-selection policy:**

1. Use the latest available valid Celsius temperature without waiting for network work on the Wayland event thread. If unavailable, retain a valid saved family/band; on first start, use the clear V29 Seascape offline fallback.
2. Approved bands and catalog family mapping are:
   - cold, `T ≤ 0°C`: V26 Winter Forest;
   - cool, `0 < T ≤ 10°C`: V19 Mountain Stream / V16 Mirror Pond;
   - mild, `10 < T ≤ 20°C`: V30 Deep Forest / VW3B Monolith Plains / V20 Crystal Caves;
   - warm, `20 < T ≤ 28°C`: V08 Jungle Waterfall / V29 Seascape / CORAL Aquarius;
   - hot, `T > 28°C`: V25 Desert.
3. Preserve the selected family while its band remains active; cross an adjacent band boundary only by more than the approved 2°C hysteresis. With no prior state, choose deterministically by best exact precipitation-variant match, then stable family ID.
4. `scene <id>` and `next` remain explicit existing user selection paths. Persist only selected family ID and temperature band in a private atomic record; never persist coordinates or weather values.
5. `reload-config`, ordinary weather temperature updates, clock/solar ticks, and output topology events do not choose a family. A successful config reload invalidates queued adaptive candidate work while retaining the resident scene.

The source catalog lists names, months, and variants, not factual climate ranges; the approved family mapping is editorial policy, not an inference from the artwork. Winter Manor is excluded because it is not animated. V12 is unavailable and must not be synthesized. V05 remains excluded pending its time-window behavior. No artwork may be tracked or published without separate provenance/license review.

## Compatibility, alternatives, and rollback

Schema-v1 and schema-v2 configuration files preserve their prior behavior. Weather remains disabled by default. Richer user-facing family, threshold, or polling customization is deferred. The adaptive registry is not activated by this change alone.

- **Do nothing:** retain v0.5 exactly; weather remains cache/status-only.
- **Minimal correction:** allow same-family precipitation variants but omit temperature-based family policy and persistence; this does not meet the approved full policy.
- **Approved policy:** add opt-in family/variant selection, metadata, private family/band persistence, and revalidate stale/failure handling, config invalidation, upload counts, multi-output resource sharing, privacy, async responsiveness, and assigned-session behavior.

Rollback disables the opt-in and retains the last valid package. Do not delete or rewrite historical evidence; keep the original S05/S07/S08/S10 acceptances as history and track CR-0006 revalidation separately.

## Approval and conditions

- Decision: `simplysoup` approved the policy above on 2026-09-23, including temperature bands, 2°C hysteresis, family mapping, same-family fallback, full-selection triggers, and family/band-only persistence. Additional directions: exclude Winter Manor; make a private Monolith Plains crop to remove the authorized blank bottom/left margins; defer richer user-facing configurability.
- Approver/date: `simplysoup` / 2026-09-23.
- **Asset clarification disposition:** source-only palette/index inspection initially did not establish a safe Monolith crop rectangle, and the V29 cloudy speed treatment was unclear. The maintainer later supplied a rough crop guide and directed a global half-speed multiplier for all animation effects under CR-0007. A private review crop was prepared from the guide and source animation rates are now retained; RB-0004 is resolved as a clarification record only. Source artwork remains unchanged, and no adaptive registry activation or acceptance is implied. See [RB-0004](../roadblocks/RB-0004-cr0006-adaptive-artwork-clarification.md) and [CR-0007](CR-0007-global-animation-speed-multiplier.md).

## Application

- Applied baseline: delivery contract v0.6, 2026-09-23; policy reconciled in the contract, scene format, IPC/config, architecture, affected S05/S07/S08/S10 criteria, release guidance, examples, roadmap, status, and README.
- Applying tree: uncommitted worktree based on `c5263e7`; no commit was made.
- Implementation update/evidence: [CR-0006 implementation update](../updates/2026-09-23-cr0006-weather-adaptive-scenes.md). It distinguishes offline code tests from missing asset, async-path, and live evidence.
- Historical evidence disposition: original S05/S07/S08/S10 acceptance records and roadmap states remain intact but do not prove the supplemental CR-0006 behaviors. The CR-0006 S05/S10 async sub-gates remain blocked by RB-0005; S07/S08 addendum work remains in progress. Acceptance of this amendment is not claimed. RB-0004's clarification is resolved, but adaptive package validation/assigned-session evidence remain open. Independent offline code/docs work continues.
- Not run in the initial CR-0006 implementation phase: adaptive Wayland/Vulkan scene swap, provider request, process launch/restart, next-login test, private config activation, private artwork edit, or commit. The later guide-based private review crop and V29 animation-manifest normalization are recorded under CR-0007/RB-0004; neither was activated.
