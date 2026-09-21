# Record templates

Copy only the template needed for the event. Replace every placeholder before submitting; never manufacture approvals, logs, or results. One short increment may combine task, evidence, update, and handoff fields when that avoids repetition.

| Template | Destination | When |
| --- | --- | --- |
| [Stage task](stage-task.md) | Task assignment/update or handoff record | Bounded implementation/delegation |
| [Evidence](evidence.md) | `docs/evidence/YYYY-MM-DD-Sxx-name.md` | Gate review or substantial validation |
| [Roadblock](roadblock.md) | `docs/roadblocks/RB-NNNN-name.md` | Work cannot proceed |
| [Roadmap change](roadmap-change.md) | `docs/changes/CR-NNNN-name.md` | Baseline/scope/sequence/gates must change |
| [Decision](decision.md) | `docs/decisions/ADR-NNNN-name.md` | Technical alternatives/rationale |
| [Failure mode](failure-mode.md) | `docs/knowledge/failures/FM-NNNN-name.md` | Reusable failure mechanism |
| [Best practice](best-practice.md) | `docs/knowledge/practices/BP-NNNN-name.md` | Reusable evidence-backed technique |
| [Handoff](handoff.md) | `docs/handoffs/YYYY-MM-DD-Sxx-name.md` | Delegation return or incomplete work transfer |

Use repository-relative text paths in copied records until you can add correct relative Markdown links. Run documentation validation after moving/copying a template. The integrator assigns/resolves sequential IDs; do not overwrite another record.
