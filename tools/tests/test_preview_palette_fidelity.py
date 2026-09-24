"""S12-AC6 preview fidelity: the curator JS twin must match the Rust goldens.

`fixtures/preview-palette-goldens.json` is generated and continuously verified
against the accepted CPU reference `Animation::apply_palette` by the Rust test
`preview_palette_goldens_match_cpu_reference_and_byte_payloads`. This test runs
the curator's JavaScript palette twin (`src/bin/curator_palette.js`, the module
the WebGL2 preview self-check compares its shader against) over the same
goldens with Node.js, closing the preview -> twin -> CPU reference -> GPU gate
chain. The check is explicitly recorded as not-run when Node.js is missing.
"""

import json
import os
import shutil
import subprocess
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
GOLDENS = REPO / "fixtures" / "preview-palette-goldens.json"
TWIN = REPO / "src" / "bin" / "curator_palette.js"

HARNESS = r"""
const fs = require("fs");
const twin = require(process.env.TWIN_PATH);
const goldens = JSON.parse(fs.readFileSync(process.env.GOLDENS_PATH, "utf8"));

function paletteFromFormula() {
  const palette = new Uint8Array(256 * 4);
  for (let i = 0; i < 256; i += 1) {
    palette[i * 4] = (i * 37) & 255;
    palette[i * 4 + 1] = (i * 91) & 255;
    palette[i * 4 + 2] = (i * 53) & 255;
    palette[i * 4 + 3] = 255;
  }
  return palette;
}

function hex(bytes) {
  return Buffer.from(bytes).toString("hex");
}

const palette = paletteFromFormula();
let failures = 0;
for (const entry of goldens.cases) {
  const metadataBytes = Uint8Array.from(Buffer.from(entry.metadata_hex, "hex"));
  const cycleBytes = Uint8Array.from(Buffer.from(entry.cycle_indices_hex, "hex"));
  for (const sample of entry.samples) {
    const output = twin.applyPalette({
      palette,
      metadataBytes,
      cycleBytes,
      time: sample.time,
      tod: null,
    });
    const actual = hex(output);
    if (actual !== sample.output_hex) {
      failures += 1;
      let first = -1;
      for (let i = 0; i < actual.length; i += 1) {
        if (actual[i] !== sample.output_hex[i]) {
          first = i;
          break;
        }
      }
      console.error(`mismatch case=${entry.name} time=${sample.time} first_hex_index=${first}`);
    }
  }
}
if (failures) {
  console.error(`${failures} golden mismatches`);
  process.exit(1);
}
console.log(`twin matches ${goldens.cases.length} golden cases`);
"""


class PreviewPaletteFidelityTest(unittest.TestCase):
    def test_goldens_fixture_is_present_and_versioned(self):
        goldens = json.loads(GOLDENS.read_text())
        self.assertEqual(goldens["schema_version"], 1)
        self.assertGreaterEqual(len(goldens["cases"]), 4)
        for case in goldens["cases"]:
            self.assertEqual(len(case["metadata_hex"]), 256 * 32 * 2)
            self.assertEqual(len(case["cycle_indices_hex"]), 256 * 4 * 2)
            self.assertGreaterEqual(len(case["samples"]), 3)

    def test_js_twin_matches_rust_reference_goldens(self):
        if shutil.which("node") is None:
            self.skipTest(
                "node is unavailable: the cross-language preview fidelity "
                "check is recorded as not-run, not passed"
            )
        environment = dict(os.environ)
        environment["TWIN_PATH"] = str(TWIN)
        environment["GOLDENS_PATH"] = str(GOLDENS)
        result = subprocess.run(
            ["node", "-e", HARNESS],
            capture_output=True,
            text=True,
            cwd=REPO,
            env=environment,
            timeout=120,
        )
        self.assertEqual(
            result.returncode,
            0,
            f"preview twin diverged from goldens:\n{result.stdout}\n{result.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
