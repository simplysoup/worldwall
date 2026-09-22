import json
import tempfile
import unittest
from pathlib import Path

from tools.run_effectgames_matrix import (
    load_config,
    parse_key_value,
    parse_registry_scene_ids,
    resolve_scenes,
)


class MatrixRunnerTests(unittest.TestCase):
    def test_reads_registry_scene_list(self):
        with tempfile.TemporaryDirectory() as temporary:
            registry = Path(temporary) / "registry.toml"
            registry.write_text(
                'schema_version = 1\nscenes = ["one", "two"]\n',
                encoding="utf-8",
            )
            self.assertEqual(parse_registry_scene_ids(registry), ["one", "two"])

    def test_cli_scenes_override_config(self):
        self.assertEqual(
            resolve_scenes({"scenes": ["config-scene"]}, ["cli-scene"], Path(".")),
            ["cli-scene"],
        )

    def test_config_loads_json_object(self):
        with tempfile.TemporaryDirectory() as temporary:
            config = Path(temporary) / "matrix.json"
            config.write_text(json.dumps({"fps": 15}), encoding="utf-8")
            self.assertEqual(load_config(config), {"fps": 15})

    def test_environment_argument_requires_valid_name(self):
        self.assertEqual(parse_key_value("WORLDWALL_FPS=30"), ("WORLDWALL_FPS", "30"))
        with self.assertRaises(Exception):
            parse_key_value("not-valid=30")


if __name__ == "__main__":
    unittest.main()
