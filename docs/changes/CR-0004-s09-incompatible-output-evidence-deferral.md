# CR-0004 — S09 incompatible-output evidence deferral

- State: applied
- Proposer / date: Pi coding agent / 2026-09-22
- Decision owner: `simplysoup`
- Baseline version / affected IDs: contract v0.3 → v0.4; S09-AC5 / WW-09 / WW-11
- Motivation / evidence: The assigned host has no physical output or adapter fixture that produces incompatible presentation capabilities. The bounded unit/source failure path and compatible-output live behavior are recorded in the [S09 implementation update](../updates/2026-09-22-s09-multi-output.md). The maintainer directed that S09 be accepted while the physical incompatible-output fixture remains future work.

## Exact delta

| Item | Before | Applied after | Reason |
| --- | --- | --- | --- |
| S09-AC5 evidence boundary | Physical incompatible-capability output evidence was an open required gate; unit/source coverage could not replace it. | The S09 baseline accepts the bounded unit/source capability-failure evidence and compatible-output live behavior. Physical incompatible-output evidence is explicitly deferred as future work and remains `not-run`; no universal hardware-compatibility claim is made. | No suitable physical fixture is available; fabricating capability results or treating a mock as hardware evidence would be misleading. |
| S09 handoff | Physical incompatible-output evidence was required before acceptance. | Future work must use an authorized physical fixture, preserve a compatible-output control, verify no second device/retry storm/selection advance, and restore topology. | Retain a concrete validation path without blocking the current delivery on unavailable hardware. |

## Alternatives and impact

- **Do nothing:** Keep S09 in progress until a physical incompatible fixture becomes available; this preserves the original gate but blocks the dependent S10 stage for an unavailable environment.
- **Smallest correction:** Accept only the bounded capability-failure path and explicitly retain the physical result as `not-run`, without weakening the runtime behavior or claiming arbitrary hardware compatibility. This is the applied choice.
- **Rejectable alternative:** Treat the unit test, a mock `SurfaceCapabilities`, or a nested compositor as physical evidence. This was rejected because it would violate the evidence policy.

The device-sharing, bounded degraded-state, no-reselection, and compatible-output behavior remain unchanged. S10 may proceed from the accepted S09 baseline. Future physical testing may add evidence or reopen the S09 gate if it finds a regression; it must not silently broaden the supported adapter matrix.

## Approval

- Decision / conditions: Approved to accept S09 with the physical incompatible-output result recorded as future work and `not-run`; retain the documented validation plan and do not claim universal compatibility.
- Approver identity / date: `simplysoup` / 2026-09-22, recorded from the maintainer instruction to accept S09 and defer this evidence.

## Application

- Applying revision / evidence: Applied in the S09 acceptance section of [the implementation update](../updates/2026-09-22-s09-multi-output.md), the roadmap ledger, status, README, contract v0.4, and the S09 stage handoff.
- Updated contract version / files: `docs/delivery-contract.md`, `docs/stages/09-multi-output.md`, `docs/roadmap.md`, `docs/status.md`, `README.md`, and the S09 update.
- Accepted evidence made stale: none; the physical fixture result remains explicitly not-run and does not replace historical unit/source evidence.
- Linked roadblock disposition: no execution roadblock is claimed; the unavailable physical fixture is retained as a future-work limitation.
