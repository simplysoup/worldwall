# FM-0014 — Python live-runner stream permission call

- Classification: resolved-with-regression-test
- Owner / date / affected versions: Pi coding agent / 2026-09-23 / uncommitted `tools/run_cr0005_live.py` live-runner version before the fix
- Stage / requirements: CR-0005 test tooling; S04-AC8, S10-AC6 live-run preparation
- Related roadblock / evidence: no roadblock; [CR-0005 test-batch record](../../updates/2026-09-23-cr0005-test-batch-run-guide.md); regression test `test_live_runner_opens_private_log_files` in `tools/tests/test_cr0005_test_batch.py`

## Trigger, symptom, and mechanism

The user invoked `tools/run_cr0005_live.py --confirm-live-run` after making the Wayland session available. The script completed preflight/build and failed while creating the frame log, before `subprocess.Popen`:

```text
AttributeError: '_io.TextIOWrapper' object has no attribute 'chmod'
```

The cause is confirmed: the runner called `.chmod()` on the Python text stream returned by `Path.open()`. Permissions belong to the filesystem `Path`, not the stream. The failed attempt did not launch a Worldwall child. The briefly created log files were inside a mode-`0700` bundle directory; the subsequent fix creates/chmods each path to `0600` before opening it.

## Detection and reproduction

The traceback was produced by the user's live-run command in the private CR-0005 bundle `cr0005-test-batch-20260923T010005Z`. The exact original terminal output was not captured by the runner; a private `runner-permission-error.log` preserves the error summary and the corresponding `live-result.json` records `launched=false`. The failure occurred before any display change or provider request.

The corrected runner was then exercised in the assigned Wayland session using `cr0005-test-batch-20260923T011740Z`; control/upload checks completed successfully. The user later reported that the synthetic test palette looked purple/all wrong, and the bundle now records that visual result as `fail`. The same frame data records sparse callbacks on one output; see the linked CR-0005 run record rather than inferring visual/pacing acceptance from process exit status.

## Recovery and prevention

Create a private empty log with the existing path-based helper, set mode `0600` on that path, then open the stream. Do not call filesystem APIs on a stream wrapper. The live runner now uses `open_private_log()` for both the application and CSV captures.

This is a test-tool defect, not a Wayland or renderer failure. The live runner checks for pre-existing Worldwall processes and the explicit authorization flag before launch; neither guard is weakened by this fix.

## Regression coverage

`test_live_runner_opens_private_log_files` checks that `open_private_log()` writes content and leaves the file mode at `0600`. It passed as part of the 49-test Python tooling run after the fix. Revalidate if Python's file APIs or the logging implementation changes.
