"""Regression tests for docs tooling only; these are not Worldwall application tests."""
from __future__ import annotations

import importlib.util
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("check_docs", ROOT / "tools/check_docs.py")
assert SPEC and SPEC.loader
checks = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checks)


def row(stage="S00", dependency="None", state="planned", owner="unassigned", evidence="None"):
    filename = {"S00": "00-foundation.md", "S01": "01-wayland-surface.md"}[stage]
    return f"| [{stage}](stages/{filename}) | Work | {dependency} | {state} | {owner} | {evidence} |"


class MarkdownTests(unittest.TestCase):
    def test_fenced_links_and_headings_are_ignored(self):
        text, errors = checks.without_fences("# Real\n```md\n# Fake\n[bad](missing.md)\n```\n")
        self.assertEqual(errors, [])
        self.assertEqual(checks.heading_anchors(text), {"real"})
        self.assertNotIn("missing.md", text)
        self.assertEqual(len(text.splitlines()), 5)

    def test_fence_kind_and_length_must_match(self):
        _, errors = checks.without_fences("````md\n```\n~~~\n")
        self.assertEqual(errors, ["line 1: unclosed code fence"])
        _, errors = checks.without_fences("~~~text\ntext\n~~~~\n")
        self.assertEqual(errors, [])

    def test_heading_slug_and_duplicate_suffix(self):
        text = "# Hello, `World`!\n## Hello, World!\n# café_name\n# Hello, World!\n"
        self.assertEqual(checks.heading_anchors(text), {"hello-world", "hello-world-1", "café_name", "hello-world-2"})

    def test_heading_anchor_collision(self):
        self.assertEqual(checks.heading_anchors("# A\n# A-1\n# A\n"), {"a", "a-1", "a-2"})


class LinkTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.source = self.root / "source.md"
        self.source.write_text("# Source", encoding="utf-8")
        self.target = self.root / "with space.md"
        self.target.write_text("# Known heading\n# Known heading\n", encoding="utf-8")
        self.docs = {self.source: "# Source", self.target: self.target.read_text(encoding="utf-8")}

    def errors(self, text):
        return checks.local_link_errors(self.root, self.source, text, self.docs)

    def test_relative_encoded_path_and_duplicate_anchor(self):
        self.assertEqual(self.errors("[ok](with%20space.md#known-heading-1)"), [])

    def test_angle_path_and_optional_title(self):
        self.assertEqual(self.errors('[ok](<with space.md#known-heading> "title")'), [])

    def test_missing_target(self):
        self.assertIn("missing link target", self.errors("[bad](missing.md)")[0])

    def test_missing_anchor(self):
        self.assertIn("missing heading anchor", self.errors("[bad](with%20space.md#absent)")[0])

    def test_same_document_anchor(self):
        self.assertEqual(self.errors("[ok](#source)"), [])
        self.assertTrue(self.errors("[bad](#not-source)"))

    def test_remote_and_inline_code_are_not_local_links(self):
        self.assertEqual(self.errors("[web](https://example.invalid/x) [mail](mailto:a@example.invalid) `[bad](missing)`"), [])

    def test_path_traversal(self):
        self.assertIn("escapes project", self.errors("[bad](../outside.md)")[0])

    def test_symlink_cannot_escape_project(self):
        with tempfile.TemporaryDirectory() as outside:
            external = Path(outside) / "external.md"
            external.write_text("# External", encoding="utf-8")
            (self.root / "escape.md").symlink_to(external)
            self.assertIn("escapes project", self.errors("[bad](escape.md)")[0])


class StageTests(unittest.TestCase):
    def stage(self):
        return "# S00 — Foundation\n\n" + "\n\n".join(
            f"## {heading}\n" + ("- [ ] **S00-AC1:** Evidence." if heading == "Acceptance criteria" else "Details.")
            for heading in checks.SECTIONS
        )

    def test_complete_stage(self):
        self.assertEqual(checks.stage_errors("S00", self.stage()), [])

    def test_missing_section_and_criteria(self):
        text = self.stage().replace("## Handoff", "## Other").replace("- [ ] **S00-AC1:**", "Unnumbered")
        errors = checks.stage_errors("S00", text)
        self.assertTrue(any("sections" in e for e in errors))
        self.assertIn("no acceptance criteria found", errors)

    def test_duplicate_or_wrong_stage_acceptance_ids(self):
        text = self.stage() + "\n- [ ] **S00-AC1:** Duplicate.\n- [ ] **S01-AC2:** Wrong."
        errors = checks.stage_errors("S00", text)
        self.assertIn("duplicate acceptance criterion ID", errors)
        self.assertIn("acceptance criterion ID does not belong to this stage", errors)

    def test_title_mismatch(self):
        self.assertTrue(checks.stage_errors("S01", self.stage()))


class RoadmapTests(unittest.TestCase):
    expected = {"S00": "00-foundation.md", "S01": "01-wayland-surface.md"}

    def test_valid_dependency_chain(self):
        text = row(state="accepted", owner="maintainer", evidence="[evidence](evidence.md)") + "\n" + row("S01", "S00", "ready")
        self.assertEqual(checks.roadmap_errors(text, self.expected), [])

    def test_unknown_dependency(self):
        errors = checks.roadmap_errors(row() + "\n" + row("S01", "S99"), self.expected)
        self.assertTrue(any("unknown dependency" in e for e in errors))

    def test_dependency_cycle(self):
        errors = checks.roadmap_errors(row(dependency="S01") + "\n" + row("S01", "S00"), self.expected)
        self.assertTrue(any("dependency cycle" in e for e in errors))

    def test_missing_and_duplicate_stages(self):
        errors = checks.roadmap_errors(row() + "\n" + row(), self.expected)
        self.assertIn("duplicate roadmap row: S00", errors)
        self.assertIn("missing roadmap stage: S01", errors)

    def test_unknown_state_and_wrong_path(self):
        text = row(state="done").replace("00-foundation.md", "wrong.md")
        errors = checks.roadmap_errors(text, {"S00": "00-foundation.md"})
        self.assertTrue(any("unknown state" in e for e in errors))
        self.assertTrue(any("unexpected stage/path" in e for e in errors))

    def test_acceptance_requires_owner_evidence_and_dependencies(self):
        errors = checks.roadmap_errors(row() + "\n" + row("S01", "S00", "accepted"), self.expected)
        for expected in ("needs an owner", "linked acceptance evidence", "requires accepted dependency"):
            self.assertTrue(any(expected in e for e in errors), errors)

    def test_malformed_ledger(self):
        errors = checks.roadmap_errors("| S00 | only two cells |", self.expected)
        self.assertTrue(any("malformed roadmap stage row" in e for e in errors))


class SkillTests(unittest.TestCase):
    def test_valid_plain_and_quoted_strings(self):
        for description in ("Do useful work.", '"Use when: testing."', "'Use when: testing.'"):
            text = f"---\nname: test-skill\ndescription: {description}\n---\n# Instructions"
            self.assertEqual(checks.skill_metadata(text, "test-skill"), ("test-skill", []))

    def test_missing_frontmatter(self):
        self.assertTrue(checks.skill_metadata("# Instructions", "test-skill")[1])

    def test_directory_mismatch_and_empty_description(self):
        _, errors = checks.skill_metadata("---\nname: other\ndescription:\n---", "test-skill")
        self.assertIn("skill name must match its directory", errors)
        self.assertTrue(any("description" in e for e in errors))

    def test_duplicate_fields_and_bad_yaml(self):
        text = "---\nname: test-skill\nname: test-skill\ndescription: invalid: mapping\n---"
        _, errors = checks.skill_metadata(text, "test-skill")
        self.assertIn("duplicate frontmatter field: name", errors)
        self.assertTrue(any("YAML-special" in e for e in errors))

    def test_multiline_and_invalid_names_rejected(self):
        text = "---\nname: Bad_Name\ndescription: >\n  multiline\n---"
        _, errors = checks.skill_metadata(text, "Bad_Name")
        self.assertIn("invalid skill name", errors)
        self.assertTrue(any("one-line" in e for e in errors))


class ProjectTests(unittest.TestCase):
    def test_actual_documentation_baseline(self):
        errors, count, skills = checks.check_project(ROOT)
        self.assertEqual(errors, [], "\n".join(errors))
        self.assertGreaterEqual(count, 50)
        self.assertGreaterEqual(skills, 7)

    def test_archive_tampering(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            self.assertTrue(checks.archive_errors(root))
            archive = root / checks.ARCHIVE
            archive.parent.mkdir(parents=True)
            shutil.copyfile(ROOT / checks.ARCHIVE, archive)
            self.assertEqual(checks.archive_errors(root), [])
            archive.write_text("changed", encoding="utf-8")
            self.assertIn("hash mismatch", checks.archive_errors(root)[0])

    def test_deleted_required_file_and_duplicate_skill(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for source in ROOT.rglob("*.md"):
                if any(part in checks.IGNORED_DIRS for part in source.relative_to(ROOT).parts):
                    continue
                target = root / source.relative_to(ROOT)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)
            (root / "docs/status.md").unlink()
            skill = root / ".agents/skills/extra/worldwall-learning/SKILL.md"
            skill.parent.mkdir(parents=True)
            shutil.copyfile(ROOT / ".agents/skills/worldwall-learning/SKILL.md", skill)
            errors, _, _ = checks.check_project(root)
            self.assertIn("missing required document: docs/status.md", errors)
            self.assertIn("duplicate skill name: worldwall-learning", errors)

    def test_cli_failure_exit_code(self):
        with tempfile.TemporaryDirectory() as folder:
            result = subprocess.run(
                [sys.executable, str(ROOT / "tools/check_docs.py"), "--root", folder],
                text=True, capture_output=True, check=False,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("missing required document", result.stdout)

    def test_cli_success_exit_code(self):
        with patch.object(sys, "argv", ["check_docs.py", "--root", str(ROOT)]):
            with patch("builtins.print") as output:
                self.assertEqual(checks.main(), 0)
                self.assertIn("PASS:", output.call_args.args[0])


if __name__ == "__main__":
    unittest.main()
