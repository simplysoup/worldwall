# Outline audit

Reviewed 2026-09-21 against the complete original document and the local filesystem. This is a design review, not a runtime/API verification.

## Evidence and progress

The initial inventory contained only `project-outline.md` (29,056 bytes). No `Cargo.toml`, source, assets, tests, or `.git` directory existed. `git status` reported that this was not a Git repository. The preserved [original](archive/project-outline.original.md) has SHA-256:

```text
3af7b9f1bab85315e6faeba73bda5ff61bee60f5ebcaa0ae512bb3f86b6a0402
```

The architecture is a useful starting point. It is not proof of any completed software stage. Avoid percent-complete estimates: use accepted gates and observable evidence instead.

## Findings and disposition

Original section numbers below refer to the archived outline.

| ID | Source | Inconsistency, gap, or inefficiency | Proposed resolution / gate |
| --- | --- | --- | --- |
| A01 | §§32–33 | Phase 1 says a visible layer appears before a rendering/buffer path exists; the MVP says static image while Phase 2 allows only a solid color. | S01 proves protocol lifecycle, not visibility; S02 proves color presentation; S03 is the static indexed-image MVP. |
| A02 | §§9–11 | `Surface<'static>` is shown without an owning lifetime proof. Display and surface destruction order are unspecified. | S02 pins APIs and documents ownership/drop order; require a narrow reviewed unsafe boundary. No copied lifetime assertion. |
| A03 | §16 | Diagram requests a frame callback after commit, risking a request that is not included in the intended commit. | Request callback before the commit/present it is meant to pace; S02 proves actual SCTK/wgpu integration order. Avoid an extra empty commit by guesswork. |
| A04 | §§7–8, 20 | `animation.png` does not define how PNG integer channels encode signed/float speed, phase, effect, or palette groups; four channels cannot automatically express all promised effects. | Use versioned authoring data and explicitly specified GPU packing; proposed `animation.toml` requires baseline approval and a layout ADR before S04. |
| A05 | §§7–8, 18 | Generic PNG decoding can expand palette indices into RGB; palette color space and alpha behavior are unstated. | S03 preserves raw indices, validates shapes, proves color-space handling and opaque output. See scene format. |
| A06 | §§5, 25 | `precipitation` is both a Boolean and a number without units; timestamps mix local display with storage. | Canonical normalized weather uses Boolean precipitation plus optional explicitly named numeric units, UTC observation/fetch times, and freshness state. S07 fixes provider mapping. |
| A07 | §§1, 5, 24, 29, 34 | Login/resume-only prose omits manual overrides elsewhere; `reload` conflates reselection and config reload. Resume may block on weather. | Define the event/command matrix, bounded asynchronous refresh, and one selection per trigger; S05, S07, S08, S10. |
| A08 | §6 | The Rust signature lacks a meaningful empty-registry/error contract and explicit borrowed return lifetime; random scoring is not reproducible. | Define deterministic scoring, injectable seed/tie-break, validated fallback, and transactional replacement in S05. Samples are conceptual, not compiling APIs. |
| A09 | §§12–15, 21–23 | Logical versus physical sizes, scale/transform, zero-size configure, hotplug, no-output state, and surface recovery are missing. | Establish one-output lifecycle in S01–S03; extend it in S09, with mandatory recovery scenarios. |
| A10 | §§9, 22 | One pipeline is assumed for all surfaces, but surface formats/capabilities can differ; shared mutable uniforms can leak the last output's dimensions. | Share scene/device resources; cache pipelines by required format and allocate distinct per-output uniform bindings or offsets. S09 checks mixed outputs. |
| A11 | §§15–16, 30–31 | Low CPU/power is aspirational, with no benchmark baseline, sampling method, or acceptance budget. FIFO alone does not enforce 30 FPS. | Callback readiness AND monotonic timer deadline; S04 pacing counters; S11 approved hardware-specific performance budget and repeatable measurements. |
| A12 | §§5, 24–25, 29 | No offline-first startup, provider/location/privacy policy, request timeout/backoff, IPC framing/security, or missing D-Bus behavior. | Bounded offline-capable environment flow, opt-in weather, same-user socket, explicit degraded states; S07–S10. |
| A13 | §§3, 32–33 | Full module tree encourages premature scaffolding; unsafe/lifecycle recovery is delayed to generic final polish. | Grow modules only as stages need them; test failure paths within every stage. Add S00 and S11 rather than an unbounded polish bucket. |
| A14 | Entire outline | Repeated architecture/policy descriptions can drift; no approvals, ownership, gate evidence, update flow, or dependency ledger. | Contract + interface docs + executable stages + one roadmap ledger; separate progress, roadblocks, and change proposals. |
| A15 | Inspiration/assets | Inspiration is not a license; no software, artwork, attribution, or redistribution decision. | Original or explicitly licensed fixtures; license/provenance gates in S00 and S11. No bundled copied demo art. |

## Recommended sequence

1. Approve the proposed baseline, assign a maintainer, and secure a real Hyprland/Vulkan test host.
2. Retire the highest integration risk first: layer-shell lifecycle → wgpu/raw handles → static indexed scene. Do not start weather to avoid a renderer problem.
3. Measure the animated baseline early in S04; do not postpone pacing mistakes until release.
4. Keep selection pure and use fake clock/weather inputs before introducing network or suspend.
5. Add multi-output behavior after the single-output owner model is proven. Avoid a single-output singleton that makes S09 a rewrite.
6. Ship only after evidence-backed recovery, privacy, licensing, and performance gates. Missing hardware evidence is pending validation, not a pass.

## Change classification

Most corrections clarify existing intent. The new stage gates, proposed `animation.toml` authoring format, explicit `reload-config` command, deterministic selection rules, and quantified validation procedure are **new proposed decisions**, not facts recovered from the original. See [ADR-0001](decisions/ADR-0001-delivery-baseline.md). Exact library versions, weather provider, animation packing, supported device matrix, and numerical resource budgets remain decisions, with deadlines in the [decision register](decisions/README.md).
