# S00 — Foundation and baseline approval

## Purpose

Make the first renderer experiment reproducible and approved. Requirement: WW-12. Dependencies: none. State/owner: [roadmap](../roadmap.md). This is not permission to scaffold the whole final application.

## Entry conditions

- Review [status](../status.md), [audit](../audit.md), and [contract](../delivery-contract.md).
- Inventory actual files/tools and preserve unrelated local work. Git was absent at the initial review; agree on repository initialization/remote ownership rather than assuming an existing history.

## Deliverables

- Assigned maintainer, reviewer/integrator, and accepted or amended ADR-0001.
- Minimal Rust application manifest, lockfile, pinned toolchain/MSRV, and executable build/test checks.
- Compatible dependency/version matrix for SCTK, Wayland backend, raw-window-handle, wgpu/Vulkan, and event-loop integration; include actual API references and compile evidence.
- A documented test host and lawful synthetic fixture plan; explicit software/artwork licensing decisions.
- Minimal CI/local check instructions, with documentation checks included.

## Execution

1. Claim S00 in the ledger. Review proposed scope changes with the maintainer; record approval identity/date or the precise requested amendment.
2. Inspect available Rust, Wayland, Vulkan, compositor, and GPU tooling. Distinguish missing local hardware from a remote host that can run acceptance.
3. Choose the smallest compatible dependency set; compile a trivial binary and event-loop dependency spike. Do not choose versions solely from archived code samples.
4. Write an ownership sketch covering connection, output/layer owners, dependent GPU surface, and shutdown. The full safety proof belongs to S02.
5. Generate tiny original index/palette reference fixtures or document their generation for S03; record author/license/hash. Do not download demo artwork for convenience.
6. Establish formatting, lint, test, build, and documentation checks. Verify commands before adding them to README/CI.
7. Record open decision deadlines, host identity/capabilities, and limitations in evidence; move S00 to review, not directly to accepted.

## Acceptance criteria

- [ ] **S00-AC1:** Maintainer/acceptance authority is named; ADR-0001 approved with recorded identity/date and reconciled baseline files.
- [ ] **S00-AC2:** Clean checkout/setup can reproduce a locked build and run documented format/lint/test checks; actual outputs and versions are retained.
- [ ] **S00-AC3:** Dependency matrix and ownership sketch identify the raw-handle boundary and a feasible compatible API path; no invented `'static` lifetime.
- [ ] **S00-AC4:** A named live Hyprland/Vulkan test environment and operator are assigned; licensing/provenance decisions cover code and test assets.
- [ ] **S00-AC5:** Documentation checks pass and a reviewer links evidence for all criteria.

## Failure and rollback

No test host, approval, compatible dependencies, or asset rights: create a specific roadblock, keep S00 unaccepted, and identify the required decision/provider. Do not substitute headless tests for a promised live host. Remove only failed spike artifacts owned by the task; retain useful compiler evidence.

## Handoff

Provide exact setup commands, pinned versions, owner/host details, unresolved decisions, and the minimal files S01 may change. A passing empty test suite proves the harness only; say so explicitly. Make S01 ready only after S00 acceptance.
