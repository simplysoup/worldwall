# Worldwall local skills

These are on-demand workflows, not global agent configuration. Each package has a standard `SKILL.md` with a name and task-specific description. Pi discovers `.agents/skills/` in a trusted project; start a new session if new skills are not yet listed. Other agents can read the same files directly. No duplicated `.pi/skills/` tree or installation dependency is needed.

| Skill | Use when | Durable output |
| --- | --- | --- |
| [worldwall-stage-execution](worldwall-stage-execution/SKILL.md) | Starting/resuming a stage or bounded implementation slice | Task, tested change, evidence, handoff |
| [worldwall-progress-update](worldwall-progress-update/SKILL.md) | Reporting real progress, reviewing/accepting a gate | Evidence/update + integrator ledger/status update |
| [worldwall-roadblock](worldwall-roadblock/SKILL.md) | Failed experiment, unavailable environment, unresolved dependency | RB record, next experiment, escalation |
| [worldwall-roadmap-change](worldwall-roadmap-change/SKILL.md) | Scope/order/acceptance/public policy must change | CR proposal; approved coordinated baseline update |
| [worldwall-learning](worldwall-learning/SKILL.md) | Capturing failure modes or best practices | FM/BP record, regression-test link, concise guardrail |
| [worldwall-delegation](worldwall-delegation/SKILL.md) | Assigning, returning, integrating, or resuming delegated work | Bounded assignment and durable handoff |
| [worldwall-renderer-review](worldwall-renderer-review/SKILL.md) | Editing/reviewing Wayland, wgpu, shader, pacing, or output lifetimes | Safety/behavior review and relevant gate evidence |

Example Pi invocation: `/skill:worldwall-stage-execution S00`. Skill commands depend on the harness settings; directly reading the linked file always works. Resolve each skill's links relative to its own directory. Execute repository commands from the root containing `AGENTS.md`, not from the skill directory.

Read only relevant references. Do not require every skill/record for a tiny change. The [documentation map](../../docs/README.md) identifies canonical sources; skills orchestrate those documents instead of maintaining another plan. Validate changes with `python3 tools/check_docs.py` from the repository root.
