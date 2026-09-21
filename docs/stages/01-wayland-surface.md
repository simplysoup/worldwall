# S01 — Single-output Wayland lifecycle

## Purpose

Prove protocol ownership and an input-free background layer lifecycle before GPU integration. Requirements: WW-01, WW-02, WW-11. Dependency: S00. State/owner: [roadmap](../roadmap.md).

## Entry conditions

S00 is accepted; use its pinned APIs and live host. Read [architecture](../architecture.md), especially configure acknowledgement and output sizing.

## Deliverables

A minimal Wayland connection/event loop, one selected output, owned `wl_surface`/layer role, configure state machine, and graceful close/disconnect path. Visibility is not claimed without a buffer; GPU content is S02.

## Execution

1. Discover required globals and report missing layer-shell or output capability clearly; dispatch events without spinning.
2. Choose one output deterministically for this stage. Keep ownership in a record that can later be stored in an output map; do not implement all of S09.
3. Create Background role, all anchors, keyboard interactivity none, empty input region, and deliberate exclusive zone. Commit initial state without content.
4. Receive configure, verify toolkit ack behavior, and store serial/logical dimensions. Track known output scale and physical-size derivation separately.
5. Handle configure with client-chosen/zero dimensions by resolving or deferring according to the protocol; do not treat zero as a valid GPU extent.
6. Handle layer close, selected-output removal, and compositor disconnect. Cancel local work and drop protocol owners in a documented order.
7. Add state-machine tests and live event traces. Defer visible input-pass-through verification until S02 provides actual content.

## Acceptance criteria

- [ ] **S01-AC1:** Live trace proves globals → role settings → initial commit → configure/ack exactly as required by the pinned toolkit/protocol.
- [ ] **S01-AC2:** Layer has no keyboard focus request and an explicit empty pointer/touch input region; configuration is inspectable in code/trace.
- [ ] **S01-AC3:** Tests cover resize, integer output scale, unresolved/zero extent, layer close, selected-output removal, and disconnect without panic or spin.
- [ ] **S01-AC4:** Idle event loop blocks on real events/deadlines; cleanup finishes without accessing destroyed owners. Evidence separates mock state tests from real compositor events.

## Failure and rollback

Missing globals or a compositor rejection produces a diagnostic and clean exit, not a hidden fallback window. Record protocol errors with versions/serial sequence, redact session data, and reproduce with the smallest layer-only path. Do not add wgpu to obscure an unresolved role/configure failure.

## Handoff

Supply the output owner type, lifecycle trace, logical/physical size assumptions, and untested scale/transform cases. S02 may attach GPU presentation only after this gate is accepted.
