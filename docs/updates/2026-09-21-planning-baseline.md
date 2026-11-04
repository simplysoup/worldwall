# 2026-09-21 — Planning baseline and developer workflows

**Type:** documentation and developer-tooling increment; proposed baseline.  
**Operator:** coding assistant acting on the project-documentation request.  
**Reviewer / acceptance authority:** unassigned; no stage acceptance requested or recorded.  
**Related requirements:** WW-12; preparation for S00-AC1 and S00-AC5, not completion of either gate.

## Starting evidence

The initial local inventory contained only `project-outline.md` (29,056 bytes), without Git metadata, application code, a build manifest, assets, or tests. The [audit](../audit.md) identifies fifteen design/delivery findings. The original is retained verbatim at [the archive](../archive/project-outline.original.md), with SHA-256 `3af7b9f1bab85315e6faeba73bda5ff61bee60f5ebcaa0ae512bb3f86b6a0402`.

## Changes delivered for review

- Replaced the monolithic entry with a historical pointer; added root README/contributor instructions and a documentation map.
- Wrote the proposed delivery contract, ownership/event/scene interfaces, twelve detailed stage plans, one roadmap ledger, honest status, validation policy, and decision deadlines.
- Separated routine progress, diagnosed roadblocks, and approval-based roadmap changes. Added lightweight templates, delegation/handoff rules, and failure-mode/best-practice registers that distinguish anticipated risks from observed incidents.
- Added seven project-local skills under `.agents/skills/`, without global configuration or dependency changes.
- Added dependency-free Python documentation validation and regression tests under `tools/`. These are not application tests or CI.

The audit and ADR identify which decisions are proposed rather than recovered from the original. No license, weather provider, dependency version, performance budget, or acceptance authority has been silently chosen. No Rust application was implemented and no repository/remote was initialized.

## Verification

Executed locally on 2026-09-21 with Python 3.14.7, Linux `7.2.4-zen2-1-zen`, x86_64, from `$HOME/Documents/worldwall`. Python 3.10 is the tooling API baseline; it was not separately executed here.

| Check | Exact command / procedure | Observed result | Outcome |
| --- | --- | --- | --- |
| Documentation structure | `python3 tools/check_docs.py` | 57 Markdown files, 12 stages, 7 skills; no findings; original archive intact | pass |
| Tool regression tests | `python3 -m unittest discover -s tools/tests -v` | 33 tests passed, including missing links/anchors, fences, invalid stages/dependencies/skills, archive tampering, and CLI exit codes | pass |
| Original preservation | `sha256sum docs/archive/project-outline.original.md` | Matches the original hash recorded above | pass |
| Semantic document review | Read contract/interfaces, all stage gates, workflows, local skills, and ledgers against the audit | Proposed decisions remain explicit; S04 baseline uses only implemented workloads; configuration/cache epochs are distinct from selection generations; no software acceptance claimed | reviewed by author, not maintainer-approved |
| Application / live hardware | No Rust, GPU, compositor, weather, suspend, or resource-benchmark commands executed | No application or test host established in this increment | not-run |

The first tool-test run caught a trailing-newline preservation bug in fence stripping (32 passed, one failed). The implementation was corrected without weakening the test; the full rerun passed. This bounded, resolved tooling issue did not block a software stage and does not require a separate roadblock record. Generated Python caches were removed; `tools/.gitignore` excludes them from future Git tracking. Final clean reruns use `PYTHONDONTWRITEBYTECODE=1` with the same commands.

Representative baseline and validator artifact identities (SHA-256; no Git revision exists):

```text
5c30349e8492ecf18de4081f5ded0ac9a42f21ca22cf01c91f0e38a5f801520e  docs/delivery-contract.md
b1c96381c19f08b9a2d546dd2d78fca9f48f2d7a2afc9c57bcdc5034d18b596b  docs/roadmap.md
f81d17a92bc0d0bd739299b3f8ea74e205ff38411b2f82d44d921c73c910bd69  docs/architecture.md
faa1762763d0d0a3423255840180cf85a7293f047fd802b9e47f73a5dd829d88  docs/scene-format.md
0746beeeb962a2085e20a5b139622c1668352c54ea0d9474073000cc761e555b  tools/check_docs.py
494420e59e88acf953f8ca7a0c3f35c97e9ac09dae0fe303284ee9b05d9167cf  tools/tests/test_check_docs.py
```

These checks validate the documentation/tooling, not Rust APIs, GPU behavior, approval, or performance. Remote links were not checked. Parser syntax limits are documented in [tooling conventions](../../tools/README.md). No external requests, background application processes, session changes, or publication occurred.

## Handoff and next action

The maintainer is explicitly **unassigned**. Assign that role, review [ADR-0001](../decisions/ADR-0001-delivery-baseline.md), and execute [S00](../stages/00-foundation.md), including licenses, compatible pinned dependencies, a minimal build/test harness, and a named live test host. Then prove S01–S03 before integrating environment services.

No confirmed execution roadblock was discovered because no software implementation was attempted. Anticipated [failure modes](../knowledge/failure-modes.md) are not fabricated incident reports. [Roadmap](../roadmap.md) remains authoritative: zero accepted software stages. Documentation checks do not approve the baseline or prove runtime feasibility.
