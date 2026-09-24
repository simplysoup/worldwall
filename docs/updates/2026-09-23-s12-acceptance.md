# S12 acceptance — AC3–AC7

**Reviewer/acceptance authority:** `simplysoup` (maintainer) · **Date:**
2026-09-23 · **Stage:** S12 — Local scene-pool curator and automatic
eligibility (post-M4) · **Recorded by:** Pi coding agent (integrator record on
explicit maintainer approval).

## Decision

The maintainer accepted **S12-AC3, S12-AC4, S12-AC5, S12-AC6, and S12-AC7**
with the unqualified approval "I AC3-7 accepted" on 2026-09-23, recorded under
the `simplysoup` identity per that identity's standing recorded instruction for
unqualified review approvals. Together with the previously accepted S12-AC1
(2026-09-23) and S12-AC2 (2026-09-23), **all S12 acceptance criteria
(AC1–AC7) are accepted** and the S12 stage moves to `accepted`.

In the same instruction the maintainer delegated the disposition of
[FM-0016](../knowledge/failures/FM-0016-nested-harness-single-output-traces.md)
to the implementer's judgement; it was resolved by repairing the preserved
S01/S02 nested harness through an explicit baseline seam without changing its
assertions (harness green again, recorded in FM-0016).

## Evidence basis reviewed

- [AC3–AC7 implementation update](2026-09-23-s12-ac3-ac7-editing-runtime-filter-preview.md):
  shared eligibility model/filter, schema-v5 one-candidate transactional
  consumption, curator editing with atomic same-user saves and eligibility
  explanations, the preview-fidelity chain (Rust goldens ↔ JS twin ↔ shader
  self-check ↔ accepted GPU readback gate), and offline transaction/privacy
  tests (129 + 50 Rust tests, 98 Python tool tests, Node.js golden check exact
  match, locked builds, docs checks).
- [AC4/AC5 live pool-mode smoke run](2026-09-23-s12-ac4-ac5-live-pool-smoke.md):
  nested real Hyprland on the assigned host, production wallpaper untouched;
  pool-filtered startup and `next` cycling, manual-ID override, no-selection
  `reload-config`, `next` no-op reason, invalid-sidecar rollback, fail-closed
  no-eligible startup, known-input match/mismatch; three consecutive passes.
- Host records on the assigned Hyprland setup (native Wayland Firefox curator
  lifecycle and rendering, in-page preview self-check exact match, close
  lifecycle) under `~/.local/state/worldwall/s12-implementation-20260923/`.

## Recorded limitations that remain open

Acceptance is recorded over the evidence above with its stated limitations;
none of these are claimed as covered and each needs separate authorization or
testing where applicable:

- Synthetic keyboard/mouse input on the host window was not run (no input
  tooling on the assigned host); input behavior was exercised in an automated
  browser session and keyboard-navigation tests instead.
- CR-0006 adaptive pool mode was not run live (it requires the weather
  provider, outside the smoke authorization); covered by offline tests.
- Physical suspend/resume and output hotplug under pool mode were not run.
- The production session is not configured for pool mode; enabling schema-v5
  pool consumption on the session is a separate, explicitly authorized change.
- License review remains open: all captured/derived artwork is `unreviewed`,
  unbundled, and unpublished. **Publishing or distributing artifacts remains a
  separate, not-yet-authorized action.**
- CR-0006 supplemental acceptance and the historical S00–S11 records are
  separate and unchanged.

## Affected files

This record, [roadmap](../roadmap.md) (S12 state/acceptance evidence),
[status](../status.md), [delivery contract](../delivery-contract.md) status
notes, and the [CR-0008 register row](../changes/README.md).
