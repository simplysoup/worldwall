"""Offline launcher tests: no compositor, provider, or real Worldwall process."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest


LAUNCHER = Path(__file__).resolve().parents[2] / "examples/worldwall-session.sh"


@unittest.skipUnless(shutil.which("flock"), "launcher requires util-linux flock")
class SessionLauncherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="worldwall session ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = {"PATH": os.environ["PATH"], "HOME": str(self.root),
                    "WAYLAND_DISPLAY": "test-wayland",
                    "XDG_RUNTIME_DIR": str(self.root / "runtime"),
                    "XDG_CONFIG_HOME": str(self.root / "config space"),
                    "XDG_DATA_HOME": str(self.root / "data space"),
                    "XDG_STATE_HOME": str(self.root / "state space")}
        Path(self.env["XDG_RUNTIME_DIR"]).mkdir(mode=0o700)
        self.config = Path(self.env["XDG_CONFIG_HOME"]) / "worldwall/worldwall.toml"
        self.config.parent.mkdir(parents=True)
        self.config.write_text("schema_version = 4\n")
        self.binary = Path(self.env["XDG_DATA_HOME"]) / "worldwall/bin/worldwall"
        self.binary.parent.mkdir(parents=True)
        self.binary.write_text(f"#!{sys.executable}\n" + """
import json, os, pathlib, time
marker = pathlib.Path(os.environ['HOME']) / 'started.json'
marker.write_text(json.dumps(dict(os.environ)))
print('frame diagnostic', flush=True)
print('test error', file=__import__('sys').stderr, flush=True)
if os.environ.get('TEST_WAIT'):
    while True:
        time.sleep(0.05)
""")
        self.binary.chmod(0o700)
        self.marker = self.root / "started.json"

    def run_launcher(self):
        return subprocess.run(["sh", str(LAUNCHER)], env=self.env,
                              capture_output=True, text=True, timeout=3)

    def test_config_paths_and_sanitized_overrides(self):
        overrides = ["WORLDWALL_SCENE_DIR", "WORLDWALL_LAYER_ONLY", "WORLDWALL_SOLID_COLOR",
                     "WORLDWALL_TEST_EXIT_AFTER_MS", "WORLDWALL_TEST_UTC_SECONDS",
                     "WORLDWALL_TEST_WITHHOLD_CALLBACK", "WORLDWALL_TEST_WITHHOLD_OUTPUT",
                     "WORLDWALL_TEST_SURFACE_ERROR"]
        self.env.update(dict.fromkeys(overrides, "unwanted"))
        result = self.run_launcher()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")
        child = json.loads(self.marker.read_text())
        self.assertEqual(child["WORLDWALL_CONFIG"], str(self.config))
        self.assertEqual(child["WAYLAND_DISPLAY"], "test-wayland")
        self.assertFalse(set(overrides) & child.keys())
        errors = Path(self.env["XDG_STATE_HOME"]) / "worldwall/session-errors.log"
        self.assertEqual(errors.read_text(), "test error\n")
        self.assertEqual(errors.stat().st_mode & 0o777, 0o600)

    def test_explicit_config_and_bounded_trace(self):
        other = self.root / "explicit config.toml"
        other.write_text("schema_version = 4\n")
        self.env["WORLDWALL_CONFIG"] = str(other)
        trace = self.root / "trace.log"
        self.env["WORLDWALL_SESSION_TRACE"] = str(trace)
        self.assertEqual(self.run_launcher().returncode, 0)
        self.assertEqual(json.loads(self.marker.read_text())["WORLDWALL_CONFIG"], str(other))
        self.assertEqual(trace.read_text(), "frame diagnostic\n")
        self.assertEqual(trace.stat().st_mode & 0o777, 0o600)

    def test_missing_session_is_rejected(self):
        for key in ("XDG_RUNTIME_DIR", "WAYLAND_DISPLAY"):
            with self.subTest(key=key):
                original = self.env.pop(key)
                result = self.run_launcher()
                self.env[key] = original
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.marker.exists())

    def test_missing_config_and_binary_are_rejected(self):
        self.config.unlink()
        self.assertNotEqual(self.run_launcher().returncode, 0)
        self.config.write_text("schema_version = 4\n")
        self.binary.unlink()
        self.assertNotEqual(self.run_launcher().returncode, 0)
        self.assertFalse(self.marker.exists())

    def test_invalid_runtime_directory_is_rejected(self):
        self.env["XDG_RUNTIME_DIR"] = str(self.root / "missing")
        self.assertNotEqual(self.run_launcher().returncode, 0)
        self.assertFalse(self.marker.exists())

    def test_duplicate_does_not_start_and_exit_releases_lock(self):
        self.env["TEST_WAIT"] = "1"
        child = subprocess.Popen(["sh", str(LAUNCHER)], env=self.env,
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            deadline = time.monotonic() + 3
            while not self.marker.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue(self.marker.exists())
            original = self.marker.stat().st_mtime_ns
            self.assertEqual(self.run_launcher().returncode, 0)
            self.assertEqual(self.marker.stat().st_mtime_ns, original)
            self.assertIsNone(child.poll())
        finally:
            child.terminate()
            child.wait(timeout=3)
        del self.env["TEST_WAIT"]
        self.marker.unlink()
        self.assertEqual(self.run_launcher().returncode, 0)
        self.assertTrue(self.marker.exists())


if __name__ == "__main__":
    unittest.main()
