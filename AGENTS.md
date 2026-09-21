# Working on Worldwall

## Read the smallest useful context

1. Read [current status](docs/status.md), [roadmap](docs/roadmap.md), and the relevant [contract](docs/delivery-contract.md) requirements.
2. Read only the target stage, its dependencies, and any linked decisions or roadblocks.
3. Load the matching [local skill](.agents/skills/README.md). Resolve skill-relative links from the skill directory; run project commands from the repository root.

This checkout began with one design document and no implementation or Git repository. Inspect the current filesystem and, if available, `git status` before acting. Never assume that proposed paths, commands, dependency APIs, or accepted gates exist.

## Non-negotiable behavior

- Preserve the separation: environment → selector → scene resources → renderer; Wayland owns presentation lifecycle.
- A weather poll, clock tick, output hotplug, or ordinary config reload must not silently reselect the scene.
- Keep large per-pixel work on the GPU after scene upload; CPU frame submission is expected.
- Treat raw Wayland handles, destruction order, configure/ack, and frame callback ordering as safety-critical. Never silence lifetime errors by inventing a `'static` owner.
- Never claim a hardware test passed from a mock, headless run, screenshot alone, or unexecuted command.
- Never remove tests, weaken acceptance, or change scope to make a stage appear complete.
- Do not fetch weather, infer location, add telemetry, publish assets, or change session configuration without the relevant configuration/authorization.

## Pick the correct update flow

| Situation | Action |
| --- | --- |
| Work progressed within approved scope | [Progress workflow](docs/workflows/development.md); evidence, stage ledger, status |
| Cannot proceed or validation is unavailable | [Roadblock workflow](docs/workflows/roadblocks.md); record cause, impact, next experiment |
| Scope, sequence, gates, dependencies, or defaults need changing | [Roadmap-change workflow](docs/workflows/roadmap-changes.md); propose, obtain approval, then update baseline |
| A failure mechanism or reusable technique is understood | [Learning workflow](docs/workflows/learning.md); distinguish suspected from reproduced |
| Work is delegated or resumed | [Delegation workflow](docs/workflows/delegation.md); bounded file ownership and a durable handoff |

Routine stage-state updates belong in the roadmap ledger; they are not permission to rewrite the plan. A roadblock is not itself a roadmap change. Use links between records rather than copying their contents.

## Definition of a useful change

- One bounded outcome with stage and requirement IDs.
- Tests for the changed behavior, including failure paths; hardware evidence where required.
- No unrelated formatting, generated files, global setup, or speculative abstractions.
- Documentation matches observed behavior and clearly marks untested assumptions.
- Run `python3 tools/check_docs.py` and `python3 -m unittest discover -s tools/tests -v` for documentation/tool changes. For Rust commands, follow [validation](docs/validation.md) only once their prerequisites exist.
- Leave a short handoff: files, commands/results, unmet gates, active roadblocks, next safe action. The integrator, not a delegated worker, accepts gates and edits shared status/roadmap ledgers.

No named maintainers, dates, approvals, performance numbers, or completed software stages may be invented. The maintainer must assign acceptance authority in S00.
