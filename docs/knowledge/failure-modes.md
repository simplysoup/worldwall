# Failure-mode index

These entries are **anticipated design risks**, not reproduced Worldwall incidents. No implementation exists yet. Promote classifications only with evidence; create a detailed `failures/FM-NNNN-short-name.md` from the [template](../templates/failure-mode.md) when investigating a concrete mechanism. Follow [learning](../workflows/learning.md).

| ID | State | Trigger / mechanism | Detection and required prevention | Stage |
| --- | --- | --- | --- | --- |
| FM-0001 | anticipated | Raw display/surface destroyed before dependent GPU surface; fabricated `'static` lifetime | Reviewed ownership/drop proof, shutdown/hotplug stress, narrow unsafe boundary | S02, S09 |
| FM-0002 | anticipated | PNG decoder expands indices or wrong gamma path changes colors | Raw index fixtures including 0/255, color reference/readback, frozen decoding contract | S03 |
| FM-0003 | anticipated | Callback requested after intended commit; scheduler spins or waits forever | Event trace, bootstrap state, fake-clock early/late/withheld callback tests | S02, S04 |
| FM-0004 | anticipated | Weather timer or late refresh response reselects wallpaper | Selection-generation assertions; cache-only completion path | S05, S07, S08 |
| FM-0005 | anticipated | Multiple outputs share mutable dimensions and all see the last write | Mixed-output render test; per-output uniform bindings/offsets | S09 |
| FM-0006 | anticipated | Lost/zero-size surface retries without bounds or teardown cancels too late | State-machine fault injection, zero-size deferral, bounded backoff and owner-safe drop | S01, S02, S09 |
| FM-0007 | anticipated | Network or decoding blocks the Wayland loop | Slow-provider/large-asset tests, bounded async work, responsive event counters | S05, S07 |
| FM-0008 | anticipated | Duplicate resume or stale scene load overwrites a newer manual selection | Coalescing and generation-order tests; transactional resource swap | S05, S08, S10 |
| FM-0009 | anticipated | Float metadata cannot encode effects; long `f32` time loses precision | Layout ADR, finite/range checks, reference vectors, wrapped-phase long-run tests | S04 |
| FM-0010 | anticipated | Unsafe socket path/unbounded IPC or asset traversal crosses trust boundary | Ownership/peer checks, size/time limits, canonical path tests; reject unsafe input | S05, S10 |
| FM-0011 | anticipated | Cache or logs expose location/credentials; error storm repeats requests/logs | Redaction tests, restrictive cache access, request/log limits, disabled-by-default weather | S07, S10 |
| FM-0012 | anticipated | Green docs/unit tests are misreported as hardware acceptance | Evidence outcome/environment review; separate proposed, implemented, verified, accepted | All |

Existing IDs reserve their mechanisms; use FM-0013 onward for genuinely new ones, or expand an existing ID into a linked detailed record. Record recovery, regression tests, and remaining limitations when an anticipated risk is actually encountered.
