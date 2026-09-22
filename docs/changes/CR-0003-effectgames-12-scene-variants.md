# CR-0003 — Complete 12-scene reference and variant manifest

- State: applied
- Proposer / date: Pi coding agent / 2026-09-22
- Decision owner: `simplysoup`
- Baseline version / affected IDs: delivery contract v0.3; S04 / S05 / S06; WW-03 / WW-04 / WW-06 / WW-08 / WW-12; S04-AC1 / S04-AC3 / S05-AC1 / S05-AC5 / S06-AC3
- Motivation / evidence: the live Effect Games Living Worlds catalog at <https://www.effectgames.com/demos/worlds/scenes.js> defines 12 distinct scene families and 22 catalog variants. CR-0002 only materialized V19, V26, and V29, so its follow-up validation scope is incomplete. The catalog was inspected on 2026-09-22; its December entry explicitly comments that the 640×480 LBM is missing.

## Problem

The implementation currently has time-of-day timeline fixtures for only three
reference scenes:

- V26 winter forest
- V19 mountain stream
- V29 seascape

Those are representative examples, not the complete catalog. The existing
scene loader can accept additional packages, but there is no durable manifest
that names the other nine scene families, distinguishes duplicate catalog names
such as V19, or records the one-to-three source variants for each family.
Without this change, a validation run cannot tell whether a missing variant is
an unavailable source, an intentionally omitted fixture, or an implementation
failure.

## Exact delta

| Item | Before | Proposed after | Reason |
| --- | --- | --- | --- |
| Reference fixture scope | CR-0002 names and materializes V19, V26, and V29 only | Define all 12 catalog scene families and all 22 source variants below | Make fixture and validation coverage complete and auditable |
| Variant identity | Catalog display/name fields are not represented as stable package IDs | Use stable family IDs plus variant IDs; retain catalog `name`, month/window, script filename, weather label, and source-specific palette remaps | V19 and V29 reuse names across variants; V05 uses separate early/late/rain entries |
| Fixture generation | Three local untracked time-of-day atlases/manifests | Permit generation of all available variants into local untracked provenance fixtures only; no art assets enter tracked delivery | Exercise the existing package/renderer path without changing licensing boundaries |
| Validation matrix | CR-0002 follow-up sequence is limited to V19/V26/V29 | Run parser/containment checks for every available package and a bounded assigned-session sequence over the complete manifest; record source-unavailable entries separately | Prevent representative fixtures from being mistaken for catalog coverage |
| Missing-source policy | No explicit status for unavailable catalog material | Mark V12 winter manor as `source-unavailable` until the missing 640×480 LBM is obtained through an authorized source; do not synthesize a pass fixture | Preserve truthful evidence and avoid inventing artwork |

## Proposed canonical manifest

This is the complete manifest derived from the catalog snapshot. It is a
change-proposal contract, not a request to bundle artwork. `source_status =
"catalog-listed"` means that the catalog names the script; it does not claim
that the response has been fetched, licensed, or materialized locally.
`weather` is the catalog variant label consumed by the fixture/selection test;
`time_window`
records a source-specific calendar window and is not an automatic selection
trigger.

```toml
schema_version = 1
source_catalog = "https://www.effectgames.com/demos/worlds/scenes.js"
source_observed = "2026-09-22"

[[scene]]
id = "v26-winter-forest"
catalog_name = "V26"
display_name = "January - Winter Forest"

[[scene.variant]]
id = "clear"
month = "01January"
script = "janclrscpt"
weather = "clear"
source_status = "catalog-listed"

[[scene.variant]]
id = "snow"
month = "01January"
script = "jansnowscpt"
weather = "snow"
source_status = "catalog-listed"

[[scene]]
id = "v19-mountain-stream"
catalog_name = "V19"
display_name = "February - Mountain Stream"

[[scene.variant]]
id = "clear"
month = "02February"
script = "febclrscpt"
weather = "clear"
source_status = "catalog-listed"

[[scene.variant]]
id = "cloudy"
month = "02February"
script = "febcldyscpt"
weather = "cloudy"
source_status = "catalog-listed"

[[scene]]
id = "vw3b-monolith-plains"
catalog_name = "VW3BASIC"
display_name = "March - Monolith Plains"

[[scene.variant]]
id = "clear"
month = "03March"
script = "MARCLRSCPT"
weather = "clear"
source_status = "catalog-listed"

[[scene]]
id = "v30-deep-forest"
catalog_name = "V30"
display_name = "April - Deep Forest"

[[scene.variant]]
id = "clear"
month = "04April"
script = "aprclrscpt"
weather = "clear"
source_status = "catalog-listed"

[[scene.variant]]
id = "rain"
month = "04April"
script = "aprrainscpt"
weather = "rain"
source_status = "catalog-listed"

[[scene]]
id = "v08-jungle-waterfall"
catalog_name = "V08"
display_name = "May - Jungle Waterfall"

[[scene.variant]]
id = "clear"
month = "05May"
script = "MAYCLRSCPT"
weather = "clear"
source_status = "catalog-listed"

[[scene.variant]]
id = "cloudy"
month = "05May"
script = "MAYCLDYSCPT"
weather = "cloudy"
source_status = "catalog-listed"

[[scene.variant]]
id = "rain"
month = "05May"
script = "MAYRAINSCPT"
weather = "rain"
source_status = "catalog-listed"

[[scene]]
id = "v20joe-crystal-caves"
catalog_name = "V20JOE"
display_name = "June - Crystal Caves"

[[scene.variant]]
id = "clear"
month = "06June"
script = "jundayscpt"
weather = "clear"
source_status = "catalog-listed"

[[scene]]
id = "v25-desert"
catalog_name = "V25"
display_name = "July - Desert"

[[scene.variant]]
id = "clear"
month = "07July"
script = "julyclearscpt"
weather = "clear"
source_status = "catalog-listed"

[[scene.variant]]
id = "cloudy"
month = "07July"
script = "julycloudyscpt"
weather = "cloudy"
source_status = "catalog-listed"

[[scene]]
id = "coral-aquarius"
catalog_name = "CORAL"
display_name = "August - Aquarius"

[[scene.variant]]
id = "clear"
month = "08August"
script = "augclrscpt"
weather = "clear"
source_status = "catalog-listed"
palette_remap = "0:[0,0,0]"

[[scene]]
id = "v29-seascape"
catalog_name = "V29"
display_name = "September - Seascape"

[[scene.variant]]
id = "clear"
month = "09September"
script = "SEPTCLRCUMSCPT"
weather = "clear"
source_status = "catalog-listed"
palette_remap = "252:[11,11,11]"

[[scene.variant]]
id = "cloudy"
month = "09September"
script = "SEPTCLDYSCPT"
weather = "cloudy"
source_status = "catalog-listed"
palette_remap = "252:[11,11,11]"

[[scene]]
id = "v05-haunted-ruins"
catalog_name = "V05AM/V05RAIN"
display_name = "October - Haunted Ruins"

[[scene.variant]]
id = "early-clear"
month = "10October"
time_window = "early"
script = "octbegclrscpt"
weather = "clear"
source_status = "catalog-listed"
palette_remap = "254:[0,0,0];0:[11,11,11]"

[[scene.variant]]
id = "late-clear"
month = "10October"
time_window = "late"
script = "octendclrscpt"
weather = "clear"
source_status = "catalog-listed"
palette_remap = "254:[0,0,0];0:[11,11,11]"

[[scene.variant]]
id = "rain"
month = "10October"
time_window = "late"
script = "octrainscpt"
weather = "rain"
source_status = "catalog-listed"
palette_remap = "254:[0,0,0];0:[11,11,11]"

[[scene]]
id = "v16-mirror-pond"
catalog_name = "V16/V16RAIN"
display_name = "November - Mirror Pond"

[[scene.variant]]
id = "clear"
month = "11November"
script = "novclrscpt"
weather = "clear"
source_status = "catalog-listed"

[[scene.variant]]
id = "rain"
month = "11November"
script = "novrainscpt"
weather = "rain"
source_status = "catalog-listed"

[[scene]]
id = "v12-winter-manor"
catalog_name = "V12BASIC"
display_name = "December - Winter Manor"

[[scene.variant]]
id = "clear"
month = "12December"
script = "DECCLRSCPT"
weather = "clear"
source_status = "source-unavailable"
source_note = "catalog comment: missing 640x480 LBM"
```

The manifest contains 12 `scene` entries and 22 `scene.variant` entries. The
three already materialized families are not special cases; they occupy their
corresponding rows above.

## Alternatives and impact

**Do nothing:** retain three representative families and explicitly document
that the other nine are out of scope. This is the smallest operational change,
but it does not satisfy the stated complete-manifest requirement and leaves
coverage ambiguous.

**Smallest correction:** add only the 12-family/22-variant manifest and mark
all unmaterialized sources `not-run`. This fixes inventory truth but does not
provide broad fixture or renderer validation.

**Proposed correction:** adopt the manifest above, fetch and generate only
authorized local untracked fixtures for catalog-listed variants whose source
and terms are verified, and validate each package through parser/registry tests.
Run one assigned-session sequence that
covers the complete available matrix while preserving the rule that weather
labels and timeline ticks never reselect a scene. Keep V12 explicitly blocked
until its source is available. No public package, network behavior, shader
selection rule, or scene-selection trigger changes.

Dependencies and order remain acyclic: the existing S04/S05/S06 loaders and
renderer precede fixture expansion; S08 and later live power/output stages are
unchanged. Resource impact is bounded by one atlas and one package per
materialized variant, with all large/art files remaining outside Git. The
validation run may be long, but no duration or performance estimate is claimed.

Compatibility impact is limited to the proposed fixture/registry manifest and
stable package IDs. Existing packages remain valid. The source-specific remaps
must be applied during authorized fixture derivation, not as hidden renderer
behavior. Rollback is deletion of the follow-up manifest and untracked derived
fixtures; CR-0002's three fixtures and optional timeline format remain valid.

Required documentation and skill updates after approval are the CR register,
CR-0002 follow-up evidence, S04/S05/S06 evidence, scene-format fixture
provenance, and any source/licensing record. Do not modify normative contract,
stage gates, roadmap state, or status while this proposal is pending.

## Approval

- Decision / conditions: Approved by `simplysoup` on 2026-09-22 in the implementation instruction; no contract or stage-gate change; keep catalog-listed source material local and untracked; retain V12 as `source-unavailable`.
- Approver identity / date: `simplysoup` / 2026-09-22.

## Application

- Applying revision / evidence: applied in the current working-tree increment; see [CR-0003 evidence](../evidence/2026-09-22-cr003-scene-variants.md).
- Updated contract version / files: delivery contract remains v0.3. Added the metadata-only manifest `fixtures/effectgames-reference-manifest.toml`, the standard-library generator `tools/generate_effectgames_fixtures.py`, local fixture provenance instructions, and CR-0003 follow-up evidence. No normative WW requirement or stage gate changed.
- Accepted evidence made stale: none. CR-0002's three-scene coverage remains historically valid and is now extended by this separately recorded complete catalog inventory.
- Linked roadblock disposition: none; missing December source is represented explicitly as `source-unavailable`, with no synthesized package or pass claim.
