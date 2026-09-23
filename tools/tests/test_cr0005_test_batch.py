"""Private-fixture and logging-safety checks for the CR-0005 test tools."""
from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / "tools"
sys.path.insert(0, str(TOOLS))
import run_cr0005_tests as batch  # noqa: E402


class FixtureTests(unittest.TestCase):
    def make_scene_source(self, root: Path) -> Path:
        source = root / "source-scene"
        source.mkdir()
        (source / "scene.toml").write_text(
            'schema_version = 1\nid = "fixture"\nimage = "scene.png"\n'
            'palette = "palette.png"\ntime_of_day = "time_of_day.toml"\n',
            encoding="utf-8",
        )
        (source / "scene.png").write_bytes(b"local scene fixture")
        (source / "palette.png").write_bytes(b"local palette fixture")
        (source / "time_of_day.toml").write_text(
            'schema_version = 1\natlas = "time_of_day.png"\n', encoding="utf-8"
        )
        (source / "time_of_day.png").write_bytes(b"old atlas")
        (source / "animation.toml").write_text("schema_version = 1\n", encoding="utf-8")
        return source

    def test_live_fixture_is_private_weather_disabled_and_does_not_touch_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = root / "operator-config.toml"
            config.write_text(
                'schema_version = 2\n[location]\nlatitude = 1.25\n'
                'longitude = 2.5\ntimezone = "America/Toronto"\n',
                encoding="utf-8",
            )
            source = self.make_scene_source(root)
            original_manifest = (source / "scene.toml").read_bytes()
            original_timeline = (source / "time_of_day.toml").read_bytes()
            bundle = root / "bundle"

            result = batch.prepare_live_fixture(bundle, config, source)

            self.assertEqual(result["weather"], "disabled")
            self.assertNotIn("1.25", json.dumps(result))
            self.assertNotIn("2.5", json.dumps(result))
            self.assertEqual((source / "scene.toml").read_bytes(), original_manifest)
            self.assertEqual((source / "time_of_day.toml").read_bytes(), original_timeline)
            self.assertEqual(Path(result["scene_dir"]).name, "effectgames-time-of-day-scene")
            copied_timeline = Path(result["scene_dir"]) / "time_of_day.toml"
            copied_atlas = Path(result["scene_dir"]) / "time_of_day.png"
            self.assertEqual(copied_timeline.read_bytes(), original_timeline)
            self.assertEqual(copied_atlas.read_bytes(), (source / "time_of_day.png").read_bytes())
            self.assertEqual(result["timeline_schema_version"], 1)
            self.assertEqual(bundle.stat().st_mode & 0o777, 0o700)
            test_config = Path(result["config_path"])
            self.assertEqual(test_config.stat().st_mode & 0o777, 0o600)
            values = batch.parse_known_toml_scalars(test_config.read_text(encoding="utf-8"))
            self.assertEqual(values[("location", "latitude")], 1.25)
            self.assertEqual(values[("location", "longitude")], 2.5)
            self.assertEqual(values[("location", "timezone")], "America/Toronto")
            self.assertEqual(values[("weather", "provider")], "disabled")

            scene_manifest = batch.parse_known_toml_scalars(
                (Path(result["scene_dir"]) / "scene.toml").read_text(encoding="utf-8")
            )
            timeline_path = Path(result["scene_dir"]) / scene_manifest[(None, "time_of_day")]
            timeline = batch.parse_known_toml_scalars(timeline_path.read_text(encoding="utf-8"))
            self.assertEqual(timeline[(None, "schema_version")], 1.0)
            self.assertNotIn((None, "basis"), timeline)
            self.assertEqual(timeline[(None, "atlas")], "time_of_day.png")
            self.assertEqual(timeline_path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(
                result["time_of_day_timeline_sha256"],
                batch.hashlib.sha256(original_timeline).hexdigest(),
            )
            self.assertEqual(
                result["time_of_day_atlas_sha256"],
                batch.hashlib.sha256((source / "time_of_day.png").read_bytes()).hexdigest(),
            )

    def test_live_runner_rejects_modified_time_of_day_atlas(self):
        live = self.load_live_runner()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = root / "operator-config.toml"
            config.write_text(
                'schema_version = 2\n[location]\nlatitude = 1.25\n'
                'longitude = 2.5\ntimezone = "America/Toronto"\n',
                encoding="utf-8",
            )
            source = self.make_scene_source(root)
            bundle = root / "bundle"
            fixture = batch.prepare_live_fixture(bundle, config, source)
            (bundle / "manifest.json").write_text(
                json.dumps({"live_fixture": fixture}), encoding="utf-8"
            )
            scene_dir = Path(fixture["scene_dir"])
            self.assertTrue(live.source_time_of_day_preserved(bundle, scene_dir))
            (scene_dir / "time_of_day.png").write_bytes(b"replacement atlas")
            self.assertFalse(live.source_time_of_day_preserved(bundle, scene_dir))

    def test_incomplete_location_and_symlink_scene_source_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = root / "incomplete.toml"
            config.write_text('schema_version = 2\n[location]\ntimezone = "Etc/UTC"\n', encoding="utf-8")
            with self.assertRaises(ValueError):
                batch.prepare_live_fixture(root / "bad-bundle", config, root)
            self.assertFalse((root / "bad-bundle").exists())

            source = self.make_scene_source(root)
            (source / "outside-link").symlink_to(config)
            good_config = root / "good.toml"
            good_config.write_text(
                'schema_version = 2\n[location]\nlatitude = 0\nlongitude = 0\ntimezone = "Etc/UTC"\n',
                encoding="utf-8",
            )
            with self.assertRaises(ValueError):
                batch.prepare_live_fixture(root / "symlink-bundle", good_config, source)

    def load_live_runner(self):
        spec = importlib.util.spec_from_file_location("cr0005_live", TOOLS / "run_cr0005_live.py")
        self.assertIsNotNone(spec)
        assert spec and spec.loader
        live = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(live)
        return live

    def test_live_runner_uses_the_same_known_scalar_parser(self):
        live = self.load_live_runner()
        self.assertTrue(callable(live.run_ctl))
        self.assertEqual(batch.parse_known_toml_scalars('fps = 30\n[("bad",)]\n')[(None, "fps")], 30.0)

    def test_live_frame_metrics_are_grouped_by_output_and_fps(self):
        live = self.load_live_runner()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "frames.csv"
            path.write_text(
                "observed_monotonic_ns,output,fps,scene_uploads,animation_uniform_updates,phase_seconds\n"
                "0,114,30,1,1,0.0\n"
                "33000000,114,30,1,2,0.1\n"
                "66000000,114,30,1,3,0.2\n"
                "100000000,115,15,1,4,0.3\n"
                "166666667,115,15,1,5,0.4\n"
                "200000000,114,15,1,6,0.5\n"
                "233000000,114,30,1,7,0.6\n"
                "266000000,114,30,1,8,0.7\n",
                encoding="utf-8",
            )
            result = live.metrics_summary(path)
            by_series = {
                (row["output"], row["fps"], row["rate_phase"]): row
                for row in result["by_output_fps"]
            }
            self.assertEqual(by_series[("114", 30, 1)]["frame_records"], 3)
            self.assertAlmostEqual(by_series[("114", 30, 1)]["interval_median_ms"], 33.0)
            self.assertEqual(by_series[("114", 30, 3)]["frame_records"], 2)
            self.assertAlmostEqual(by_series[("115", 15, 1)]["nominal_interval_ms"], 1000 / 15)
            self.assertEqual(result["scene_upload_values"], [1])

    def test_live_runner_opens_private_log_files(self):
        live = self.load_live_runner()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "capture.log"
            with live.open_private_log(path, mode="w", encoding="utf-8") as stream:
                stream.write("captured test output\n")
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(path.read_text(encoding="utf-8"), "captured test output\n")

    def test_live_result_uses_explicit_success_checks_not_raw_false_observations(self):
        live = self.load_live_runner()
        checks = {name: True for name in live.REQUIRED_LIVE_CHECKS}
        checks["reload_to_15_reselected"] = False
        checks["reload_to_30_reselected"] = False
        self.assertEqual(live.failed_checks(checks), [])
        checks["source_time_of_day_preserved"] = False
        self.assertEqual(live.failed_checks(checks), ["source_time_of_day_preserved"])

    def test_live_runner_refuses_preexisting_worldwall_and_requires_confirmation(self):
        live = self.load_live_runner()
        with tempfile.TemporaryDirectory() as temporary:
            bundle = Path(temporary) / "bundle"
            (bundle / "effectgames-time-of-day-scene").mkdir(parents=True)
            (bundle / "test-config.toml").write_text("weather disabled\n", encoding="utf-8")
            with patch.object(sys, "argv", ["run_cr0005_live.py", "--bundle", str(bundle), "--confirm-live-run"]):
                with patch.object(live, "worldwall_processes", return_value=[{"pid": 12, "executable_name": "worldwall"}]):
                    self.assertEqual(live.main(), 3)
            blocked = json.loads((bundle / "live-result.json").read_text(encoding="utf-8"))
            self.assertEqual(blocked["state"], "blocked-active-worldwall")
            self.assertFalse(blocked["launched"])

            with patch.object(sys, "argv", ["run_cr0005_live.py", "--bundle", str(bundle)]):
                with patch.object(live, "worldwall_processes", return_value=[]):
                    self.assertEqual(live.main(), 4)
            awaiting = json.loads((bundle / "live-result.json").read_text(encoding="utf-8"))
            self.assertEqual(awaiting["state"], "awaiting-explicit-session-authorization")
            self.assertFalse(awaiting["launched"])


if __name__ == "__main__":
    unittest.main()
