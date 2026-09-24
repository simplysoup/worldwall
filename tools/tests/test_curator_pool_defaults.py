"""The curator's missing-sidecar defaults must preserve each source type."""

import os
import shutil
import subprocess
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
MODULE = REPO / "src" / "bin" / "curator_pools.js"


class CuratorPoolDefaultsTest(unittest.TestCase):
    def test_default_pools_group_by_source_and_collection(self):
        if shutil.which("node") is None:
            self.skipTest("node is unavailable: curator pool-default check is not-run")
        script = r"""
const assert = require("assert");
const { defaultPools } = require(process.env.CURATOR_POOLS_MODULE);
const pools = defaultPools([
  { id: "canvas-a", source: "library", source_collection: "canvascycle" },
  { id: "canvas-b", source: "library", source_collection: "canvascycle" },
  { id: "registry-a", source: "registry", source_collection: "registry" },
  { id: "library-registry", source: "library", source_collection: "registry" },
  { id: "derived-a", source: "library", source_collection: "foo/bar" },
  { id: "derived-b", source: "library", source_collection: "foo?bar" },
]);
assert.deepStrictEqual(pools, [
  { id: "library-canvascycle", source: "library", enabled: true, scene_ids: ["canvas-a", "canvas-b"] },
  { id: "registry-registry", source: "registry", enabled: true, scene_ids: ["registry-a"] },
  { id: "library-registry", source: "library", enabled: true, scene_ids: ["library-registry"] },
  { id: "library-foo-bar", source: "library", enabled: true, scene_ids: ["derived-a"] },
  { id: "library-foo-bar-2", source: "library", enabled: true, scene_ids: ["derived-b"] },
]);
console.log("source-aware default pools verified");
"""
        result = subprocess.run(
            ["node", "-e", script],
            capture_output=True,
            text=True,
            cwd=REPO,
            env={**os.environ, "CURATOR_POOLS_MODULE": str(MODULE)},
            timeout=30,
        )
        self.assertEqual(result.returncode, 0, f"{result.stdout}\n{result.stderr}")


if __name__ == "__main__":
    unittest.main()
