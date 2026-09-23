# FM-0015 — Synthetic solar atlas overrides source palette

- Classification: reproduced
- Owner / date / affected versions: Pi coding agent / 2026-09-23 / uncommitted CR-0005 live-fixture generator before the source-preservation correction
- Stage / requirements: CR-0005 test tooling; S04-AC8 visual evidence
- Related roadblock / evidence: no roadblock; [CR-0005 run record](../../updates/2026-09-23-cr0005-test-batch-run-guide.md); tests `test_live_fixture_is_private_weather_disabled_and_does_not_touch_source` and `test_live_runner_rejects_modified_time_of_day_atlas`

## Trigger, symptom, and mechanism

The assigned-session test was run with a private copy of Effect Games V29. Its fixture builder rewrote the source `time_of_day.toml` from civil timeline v1 to a four-keyframe solar-v2 timeline and replaced the copied `time_of_day.png` with a generated four-row synthetic atlas. The running renderer correctly used that atlas for the whole indexed scene. The app trace sampled rows 3→0; those rows were blue-heavy. The user reported that all colors looked wrong/purple. No screenshot was captured.

The mechanism is confirmed by the fixture generator and run trace. The purple appearance is consistent with the synthetic palette data; it is not evidence that the original V29 palette is purple or that the shader incorrectly recolors it. Whether the same appearance persists with the normal V29 package was not established by that run.

## Detection and reproduction

The retained run bundle is `~/.local/state/worldwall/cr0005-test-batch-20260923T011740Z`. `worldwall-live.log` reports `trace solar-palette sample=rows lower_row=3 upper_row=0`; `solar-v2-scene/time_of_day.png` is the generated fixture atlas. The user supplied the visual failure report. The original Effect Games `time_of_day.toml` and `time_of_day.png` in the private V29 copy match the local provenance fixture byte-for-byte; the fixture swap occurred only in the test bundle.

## Recovery and prevention

Do not use an artificial atlas as a visual stand-in for an authored Effect Games scene. The live-fixture generator now copies the source schema-v1 civil-time timeline and graded palette atlas unchanged, records their hashes, and the live runner refuses to launch if either differs. The synthetic solar-v2 asset was removed from the live preparation path; solar-v2 remains covered by dedicated CPU/GPU tests.

The corrected fixture was displayed in a later guarded session attempt; the user reported the original colors looked good. The command was aborted before finalization, so no complete technical run is claimed. The private test config was restored and no Worldwall process remained. Do not launch again without the user's explicit safe-session authorization.

## Regression coverage

The 49-test Python tooling suite checks byte-for-byte source timeline/atlas preservation and verifies the live runner rejects a modified atlas. `python3 -m unittest discover -s tools/tests -v` passed after this correction. Revalidate if fixture generation or the Effect Games source package changes.
