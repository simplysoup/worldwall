# 2026-10-01 — S12 curator default pools

- Date / operator: 2026-10-01 / Pi coding agent
- Revision: working-tree increment on `main`, based on `fe42865000d7`; not committed
- Code hashes: `src/bin/curator.html` `ec6db4bd3429313822c04480929cf3c2b63625349df72924853f4578b53c9a4e`; `src/bin/curator.js` `10965535e388a7ec09ba7447f32fb0d437d03874c363cace08b30882f1e485d4`; `src/bin/curator_pools.js` `82c2236a5310ac807c038ea43245af647016e587223a13adaa25351caa7b56ee`; test `tools/tests/test_curator_pool_defaults.py` `f295f404111a833fb7079c2c59e2e871075a367acd64c8ca8508ed7adc3bd350`
- Stage / requirements: S12-AC3, S12-AC5; WW-06, WW-10
- Kind: offline implementation and validation

## Change

When no sidecar exists, the curator now starts with an unsaved suggested draft
that puts every currently loaded library and configured-registry scene into an
enabled source-collection pool, with no extra per-scene criteria. For an
existing sidecar, an explicit **Add unassigned scene(s)** action adds newly
available scenes while preserving existing pool switches and scene rules. New
pools default enabled. Users still explicitly save the sidecar, configure
`scene_pools_file` if needed, and request config reload; opening/editing the
curator does not alter the running scene or enable pool mode.

## Results

| Check | Command | Outcome |
| --- | --- | --- |
| Default grouping and incremental membership, including preservation of disabled pools and no mutation of the input | `python3 -m unittest tools.tests.test_curator_pool_defaults -v` | pass |
| JavaScript syntax | `node --check src/bin/curator.js && node --check src/bin/curator_pools.js` | pass |
| Rust formatting and Clippy | `cargo fmt --all --check`; `cargo clippy --offline --all-targets --all-features -- -D warnings` | pass |
| Rust all-target tests | `CARGO_TARGET_DIR=/tmp/worldwall-curator-validation-target cargo test --offline --all-targets --all-features` | pass: 132 main, 78 CLI, 3 example tests; 3 Vulkan tests ignored |
| Documentation and Python tools | `python3 tools/check_docs.py`; `python3 -m unittest discover -s tools/tests -v` | pass: 285 Markdown files; 117 tests |
| Whitespace | `git diff --check` | pass |

The first Rust test attempt using the checkout's existing `target/` failed in
the preview-golden test because that cached test binary had a fixture path
baked from a different checkout. A fresh temporary target directory rebuilt
from this checkout and passed all runnable Rust tests; no project fixture was
changed.

## Limitations and next action

No curator browser/Hyprland interaction test, live `worldwallctl next`, sidecar
save/reload against the user's active session, or artwork/license review was
performed. The running Worldwall process, private config, and active scene were
not changed. S12 remains `review` pending maintainer re-review of the prior
AC5 correction; this increment does not accept S12-AC3/AC5. Next: maintainer
review the defaults and, if useful, try **Add unassigned scene(s)**, save, and
explicitly reload during an authorized session window.
