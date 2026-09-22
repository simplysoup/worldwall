import tempfile
import unittest
from pathlib import Path

from tools.package_release import artifact_digest, file_entries, reject_artwork_or_fixture_paths


class ReleasePackageTests(unittest.TestCase):
    def test_artifact_digest_is_ordered_and_repeatable(self):
        entries = [
            {"path": "bin/worldwall", "bytes": 3, "sha256": "aaa"},
            {"path": "README.md", "bytes": 4, "sha256": "bbb"},
        ]
        first = artifact_digest(entries)
        self.assertEqual(first, artifact_digest(entries))
        self.assertNotEqual(first, artifact_digest(list(reversed(entries))))

    def test_file_entries_are_sorted_and_include_hashes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "z.txt").write_text("z", encoding="utf-8")
            (root / "a.txt").write_text("a", encoding="utf-8")
            entries = file_entries(root)
            self.assertEqual([entry["path"] for entry in entries], ["a.txt", "z.txt"])
            self.assertEqual(entries[0]["bytes"], 1)
            self.assertEqual(len(entries[0]["sha256"]), 64)

    def test_artwork_and_fixture_paths_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "fixtures").mkdir()
            (root / "fixtures" / "scene.png").write_bytes(b"not an asset")
            with self.assertRaises(Exception):
                reject_artwork_or_fixture_paths(root)

    def test_operator_package_paths_are_allowed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "bin").mkdir()
            (root / "bin" / "worldwall").write_bytes(b"binary")
            (root / "docs").mkdir()
            (root / "docs" / "release.md").write_text("runbook", encoding="utf-8")
            reject_artwork_or_fixture_paths(root)


if __name__ == "__main__":
    unittest.main()
