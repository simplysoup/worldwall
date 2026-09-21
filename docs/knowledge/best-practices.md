# Best-practice index

These are **recommended-unvalidated in Worldwall**, derived from the design review. They are not empirical performance or reliability claims. Some reflect proposed contract requirements; approval/verification remains in the relevant stage. Expand substantial examples into `practices/BP-NNNN-short-name.md` using [the template](../templates/best-practice.md).

| ID | Practice and when to use it | Why / limits | Validate in |
| --- | --- | --- | --- |
| BP-0001 | Prove layer → solid color → indexed image before environment services | Isolates the highest-risk integration; do not scaffold the final tree first | S01–S03 |
| BP-0002 | Keep selector pure with fake clock/weather and injected RNG | Repeatable boundary/race tests; still needs live orchestration coverage | S05–S08 |
| BP-0003 | Validate/load/upload candidate, then atomically swap resources | Preserves known-good rendering; needs generation checks for asynchronous work | S05, S10 |
| BP-0004 | Combine callback readiness and monotonic deadlines per output | Bounds work without spin; validate actual compositor callback behavior | S04, S09 |
| BP-0005 | Share immutable scene resources, isolate output-local uniforms/lifetimes | Avoids duplicated assets and last-write corruption; formats can need separate pipelines | S02, S09 |
| BP-0006 | Preserve a tiny lawful fixture/reference-vector suite | Makes color/index/animation errors reproducible; fixture correctness must be reviewed | S03, S04 |
| BP-0007 | One bounded worker outcome and one writer for shared ledgers | Reduces conflicts and vague handoffs; integrator still reviews/reruns checks | Every increment |
| BP-0008 | Separate diagnosis from scope decisions | A blocker explains inability to proceed; only approved changes rewrite acceptance | Every blocked gate |
| BP-0009 | Measure against the same host/workload before claiming improvement | Avoids misleading CPU/power statements; unmeasurable counters remain unavailable | S04, S11 |
| BP-0010 | Read status/target stage first; load detailed references on demand | Keeps working context small; do not omit linked safety decisions needed by the task | Every increment |

On validation, link actual evidence and record limitations/date/version. Use BP-0011 onward for new practices or expand existing IDs. Update related local skills with a short pointer rather than copying this catalogue into every skill.
