# RB-0004 — CR-0006 adaptive artwork inputs need clarification

- State: resolved (clarifications applied; adaptive acceptance remains open)
- Opened / last updated: 2026-09-23
- Stage / blocked criteria: CR-0006 rollout; S05-AC7, S07-AC7, S08-AC6 assigned-session adaptive swap evidence
- Owner / escalation target: Pi coding agent for offline work; `simplysoup` for asset-treatment clarification
- Kind / impact: decision/dependency; normal
- Next check: keep the validated crop private and use only after any separately authorized runtime session; CR-0006 assigned-session gates remain independent

## Expected versus observed

CR-0006 approval requests a private Monolith Plains crop removing blank bottom/left margins and requires the cloudy V29 sibling's animation-speed treatment to be clarified before adaptive use. Inspection of the source Monolith fixture found a 640×480 indexed image with index 255 along x=0–2 for y=22–479 and along x=0–348 on the bottom row; the remaining pixels contain other indices, and a large index-224 region is not a verified crop boundary. The source has no transparency metadata. Palette/index evidence therefore does not establish an exact safe rectangle. No source fixture or private derived asset was changed.

The private V29 clear copy uses halved BlendShift speeds, while the V29 cloudy sibling retains its source speeds. No decision has identified which treatment the cloudy sibling should use. Do not include it in an adaptive registry until clarified.

## Diagnosis and attempts

| Hypothesis | Experiment | Result / evidence | Conclusion |
| --- | --- | --- | --- |
| Palette/index labels establish the full blank margin boundary. | Inspect the source Monolith image and palette/index distribution. | Edge runs occur only in part of the requested bottom/left margins; other indices remain, and the large index-224 region is not a documented blank edge. | Rejected as sufficient evidence; exact crop rectangle cannot be inferred safely. |
| Existing V29 clear speed treatment can be assumed for the cloudy sibling. | Compare the private clear-copy animation manifest with the cloudy source sibling. | Clear copy has halved BlendShift speeds; cloudy sibling retains source speeds. | Unknown user intent; no automatic copy or normalization. |

No crop, asset rewrite, registry activation, or live adaptive swap had been attempted when this roadblock was opened. The maintainer later supplied a rough crop guide and directed one global half-speed scalar for all animation effects. The source fixture/guide remain unchanged; the resulting private review crop and animation normalization are documented below.

## Workaround and escalation

Keep the source fixture and crop guide unchanged, and keep adaptive private config activation off. The maintainer's rough guide clarifies the intended retained region; this implementation chose its interior integer edges and recorded the half-open convention so the private review artifact is reproducible. The global speed multiplier supersedes per-sibling speed edits: source-authored rates are retained and the configured scalar applies uniformly. Removing unresolved package variants from a private registry is not treated as satisfying rollout acceptance. The clarification roadblock is resolved by the maintainer direction and private artifact preparation; subsequent package, async, visual, and assigned-session gates remain independent.

## Resolution evidence

The maintainer supplied `scene-crop-red.png` as a rough guide and requested a global animation-speed multiplier, defaulting to approximately half speed for all effects. A private review package was created in a private review package outside the repository using the Pillow half-open rectangle `(18, 30, 346, 476)`, producing a 328×446 grayscale/index image. Its palette, time-of-day, and animation source assets were copied unchanged; the tracked/untracked source fixture and guide were not modified. The source-speed V29 animation manifest replaced the previous private halved manifest so the global default is applied once; the previous manifest is preserved in private state in a private state backup outside the repository. On 2026-09-23, `simplysoup` visually reviewed the private crop and said it looks good. A private activation registry outside the repository contains only `vw3b-monolith-plains--clear` and uses it as fallback; its `family` key follows the parser schema. A temporary local harness called `AppConfig::load_prepared` against the active private config and reported `328x446`, four animation groups, and loader success. The private config change is limited to `registry_dir`; schema 2 keeps adaptive mode off. Its pre-activation copy is retained in private state outside the repository. No process was started and no weather request or next-login test was made.

This resolves the missing asset/speed direction, crop visual review, and private ordinary-registry loader validation only. A one-scene private registry now names the crop as its fallback, and the private app config points `registry_dir` to that registry while leaving adaptive mode off. The review guide, source fixture, review-crop pixels, and copied resource images remain unchanged. The activation copy's manifest key was normalized to the application schema's `family` spelling after the first loader attempt rejected `family_id`; no source artwork was edited. No Worldwall process, adaptive swap, provider request, or next-login behavior was run. S05/S07/S08/CR-0006 acceptance remains open, and RB-0005 is tracked separately.

## Related records

- [CR-0006](../changes/CR-0006-weather-adaptive-scene-selection.md)
- [CR-0006 implementation update](../updates/2026-09-23-cr0006-weather-adaptive-scenes.md)
