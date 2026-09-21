#!/usr/bin/env python3
"""Dependency-free checks for Worldwall's deliberately simple Markdown conventions.

Not a CommonMark/YAML parser or an approval checker. See tools/README.md.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
from pathlib import Path
from urllib.parse import unquote, urlsplit

STAGE_FILES = (
    "00-foundation.md", "01-wayland-surface.md", "02-gpu-presentation.md",
    "03-indexed-scene.md", "04-palette-animation.md", "05-scene-registry.md",
    "06-time-astronomy.md", "07-weather.md", "08-suspend-resume.md",
    "09-multi-output.md", "10-ipc-config.md", "11-release-validation.md",
)
SECTIONS = (
    "Purpose", "Entry conditions", "Deliverables", "Execution",
    "Acceptance criteria", "Failure and rollback", "Handoff",
)
REQUIRED_DOCS = (
    "README.md", "AGENTS.md", "project-outline.md", "docs/README.md",
    "docs/audit.md", "docs/delivery-contract.md", "docs/architecture.md",
    "docs/scene-format.md", "docs/roadmap.md", "docs/status.md",
    "docs/validation.md", "docs/decisions/ADR-0001-delivery-baseline.md",
    "docs/knowledge/failure-modes.md", "docs/knowledge/best-practices.md",
    ".agents/skills/README.md",
)
WORKFLOWS = ("development", "roadblocks", "roadmap-changes", "learning", "delegation")
REGISTERS = ("decisions", "roadblocks", "changes", "evidence", "updates", "handoffs", "templates")
TEMPLATES = (
    "stage-task", "evidence", "roadblock", "roadmap-change", "decision",
    "failure-mode", "best-practice", "handoff",
)
SKILLS = (
    "stage-execution", "progress-update", "roadblock", "roadmap-change",
    "learning", "delegation", "renderer-review",
)
STATES = {"planned", "ready", "in-progress", "blocked", "review", "accepted"}
ARCHIVE = "docs/archive/project-outline.original.md"
ARCHIVE_SHA256 = "3af7b9f1bab85315e6faeba73bda5ff61bee60f5ebcaa0ae512bb3f86b6a0402"
IGNORED_DIRS = {".git", ".venv", "venv", "node_modules", "target", "__pycache__"}
HEADING = re.compile(r"^ {0,3}(#{1,6})\s+(.+?)\s*#*\s*$", re.MULTILINE)
# Inline links/images, with optional title; paths with parentheses use <...>.
LINK = re.compile(r'!?\[([^\]\n]*)\]\(\s*(?:<([^>\n]+)>|([^\s()]+))(?:\s+"[^"\n]*")?\s*\)')
INLINE_CODE = re.compile(r"(`+).*?\1")


def without_fences(text: str) -> tuple[str, list[str]]:
    """Keep line numbers stable; only unindented/three-space fenced blocks are supported."""
    lines = []
    fence = None
    start = 0
    for number, line in enumerate(text.splitlines(), 1):
        if fence:
            lines.append("")
            if re.fullmatch(r" {0,3}" + re.escape(fence[0]) + "{" + str(len(fence)) + r",}\s*", line):
                fence = None
        else:
            match = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line)
            if match:
                fence, start = match.group(1), number
                lines.append("")
            else:
                lines.append(line)
    visible = "\n".join(lines) + ("\n" if text.endswith("\n") else "")
    return visible, ([f"line {start}: unclosed code fence"] if fence else [])


def heading_anchors(text: str) -> set[str]:
    used: set[str] = set()
    for match in HEADING.finditer(text):
        title = LINK.sub(lambda link: link.group(1), match.group(2))
        title = re.sub(r"<[^>]*>", "", title).lower()
        base = re.sub(r"[^\w\- ]", "", title).replace(" ", "-")
        slug, suffix = base, 0
        while slug in used:
            suffix += 1
            slug = f"{base}-{suffix}"
        used.add(slug)
    return used


def local_link_errors(root: Path, source: Path, text: str, documents: dict[Path, str]) -> list[str]:
    errors = []
    for number, line in enumerate(text.splitlines(), 1):
        for match in LINK.finditer(INLINE_CODE.sub("", line)):
            destination = match.group(2) or match.group(3)
            try:
                url = urlsplit(destination)
            except ValueError:
                errors.append(f"line {number}: invalid link {destination!r}")
                continue
            if url.scheme or url.netloc:
                continue
            target = (source.parent / unquote(url.path)).resolve() if url.path else source
            if not target.is_relative_to(root):
                errors.append(f"line {number}: link escapes project: {destination}")
            elif not target.exists():
                errors.append(f"line {number}: missing link target: {destination}")
            elif url.fragment and target.suffix.lower() == ".md":
                if target not in documents:
                    errors.append(f"line {number}: Markdown anchor target not scanned: {destination}")
                elif unquote(url.fragment) not in heading_anchors(documents[target]):
                    errors.append(f"line {number}: missing heading anchor: {destination}")
    return errors


def stage_errors(stage_id: str, text: str) -> list[str]:
    errors = []
    if not text.startswith(f"# {stage_id} — "):
        errors.append(f"expected title '# {stage_id} — ...'")
    headings = [m.group(2) for m in HEADING.finditer(text) if m.group(1) == "##"]
    if headings != list(SECTIONS):
        errors.append("stage sections must appear once in the standard order")
    criteria = re.findall(r"^- \[[ xX]\] \*\*(S\d{2}-AC\d+):\*\*", text, re.MULTILINE)
    if not criteria:
        errors.append("no acceptance criteria found")
    if len(criteria) != len(set(criteria)):
        errors.append("duplicate acceptance criterion ID")
    if any(not re.fullmatch(re.escape(stage_id) + r"-AC[1-9]\d*", item) for item in criteria):
        errors.append("acceptance criterion ID does not belong to this stage")
    return errors


def roadmap_errors(text: str, expected: dict[str, str]) -> list[str]:
    errors = []
    rows: dict[str, tuple[list[str], str, str, str]] = {}
    for line in text.splitlines():
        if not re.match(r"^\|\s*\[?S\d", line):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        match = re.fullmatch(r"\[(S\d{2})\]\(([^)]+)\)", cells[0])
        if len(cells) != 6 or not match:
            errors.append(f"malformed roadmap stage row: {line}")
            continue
        stage, path = match.groups()
        if stage in rows:
            errors.append(f"duplicate roadmap row: {stage}")
        if stage not in expected or path != f"stages/{expected.get(stage)}":
            errors.append(f"unexpected stage/path: {stage} -> {path}")
        deps = cells[2]
        if deps != "None" and not re.fullmatch(r"S\d{2}(?:, S\d{2})*", deps):
            errors.append(f"{stage}: dependencies must be None or comma-separated stage IDs")
        dependencies = [] if deps == "None" else deps.split(", ")
        state, owner, evidence = cells[3:]
        if state not in STATES:
            errors.append(f"{stage}: unknown state {state!r}")
        if state in {"in-progress", "blocked", "review", "accepted"} and owner in {"", "unassigned", "None"}:
            errors.append(f"{stage}: active stage needs an owner")
        if state == "accepted" and not LINK.search(evidence):
            errors.append(f"{stage}: accepted stage needs linked acceptance evidence")
        rows[stage] = (dependencies, state, owner, evidence)
    for stage in sorted(expected.keys() - rows.keys()):
        errors.append(f"missing roadmap stage: {stage}")
    for stage, (dependencies, state, _, _) in rows.items():
        for dependency in dependencies:
            if dependency not in rows:
                errors.append(f"{stage}: unknown dependency {dependency}")
            elif state in {"ready", "in-progress", "review", "accepted"} and rows[dependency][1] != "accepted":
                errors.append(f"{stage}: {state} requires accepted dependency {dependency}")
    visiting: set[str] = set()
    visited: set[str] = set()

    def visit(stage: str) -> None:
        if stage in visiting:
            errors.append(f"dependency cycle involving {stage}")
            return
        if stage in visited:
            return
        visiting.add(stage)
        for dependency in rows[stage][0]:
            if dependency in rows:
                visit(dependency)
        visiting.remove(stage)
        visited.add(stage)

    for stage in rows:
        visit(stage)
    return errors


def skill_metadata(text: str, directory: str) -> tuple[str, list[str]]:
    """Accept only flat one-line YAML strings (plain, single-quoted, or JSON double-quoted)."""
    lines = text.splitlines()
    if not lines or lines[0] != "---" or "---" not in lines[1:]:
        return "", ["missing or unclosed skill YAML frontmatter"]
    fields = {}
    errors = []
    for line in lines[1:lines.index("---", 1)]:
        if not line.strip() or line.startswith("#"):
            continue
        match = re.fullmatch(r"([a-z][a-z-]*):\s*(.*)", line)
        if not match:
            errors.append("frontmatter must use flat one-line string fields")
            continue
        key, value = match.groups()
        if key in fields:
            errors.append(f"duplicate frontmatter field: {key}")
        if value.startswith('"'):
            try:
                value = json.loads(value)
            except ValueError:
                errors.append(f"invalid double-quoted string: {key}")
                value = ""
        elif value.startswith("'"):
            if not re.fullmatch(r"'(?:[^']|'')*'", value):
                errors.append(f"invalid single-quoted string: {key}")
                value = ""
            else:
                value = value[1:-1].replace("''", "'")
        elif value and (value[0] in "[]{}>|&*!%@`" or ": " in value or " #" in value or value in {"null", "true", "false", "~"}):
            errors.append(f"quote YAML-special values or use a plain one-line string: {key}")
            value = ""
        fields[key] = value
    name = fields.get("name", "")
    description = fields.get("description", "")
    if not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", name) or len(name) > 64:
        errors.append("invalid skill name")
    if name != directory:
        errors.append("skill name must match its directory")
    if not description.strip() or len(description) > 1024 or description.isnumeric():
        errors.append("skill description must be a nonempty string of at most 1024 characters")
    return name, errors


def archive_errors(root: Path) -> list[str]:
    archive = root / ARCHIVE
    if not archive.is_file():
        return [f"missing original archive: {ARCHIVE}"]
    if hashlib.sha256(archive.read_bytes()).hexdigest() != ARCHIVE_SHA256:
        return [f"{ARCHIVE}: original archive hash mismatch; do not rewrite history"]
    return []


def check_project(root: Path) -> tuple[list[str], int, int]:
    root = root.resolve()
    errors = []
    documents = {}
    raw = {}
    paths = []
    for directory, subdirs, files in os.walk(root):
        subdirs[:] = sorted(name for name in subdirs if name not in IGNORED_DIRS)
        paths.extend(Path(directory) / name for name in files if name.endswith(".md"))
    for path in sorted(paths):
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeError) as exc:
            errors.append(f"{path.relative_to(root)}: cannot read UTF-8: {exc}")
            continue
        raw[path] = text
        documents[path], problems = without_fences(text)
        errors.extend(f"{path.relative_to(root)}: {problem}" for problem in problems)
    required = list(REQUIRED_DOCS)
    required += [f"docs/workflows/{name}.md" for name in WORKFLOWS]
    required += [f"docs/{name}/README.md" for name in REGISTERS]
    required += [f"docs/templates/{name}.md" for name in TEMPLATES]
    required += [f".agents/skills/worldwall-{name}/SKILL.md" for name in SKILLS]
    for name in required:
        if root / name not in documents:
            errors.append(f"missing required document: {name}")
    for path, text in documents.items():
        errors.extend(f"{path.relative_to(root)}: {e}" for e in local_link_errors(root, path, text, documents))
    expected = {f"S{index:02d}": name for index, name in enumerate(STAGE_FILES)}
    actual = {path.name for path in (root / "docs/stages").glob("*.md")}
    for name in sorted(actual - set(STAGE_FILES)):
        errors.append(f"unregistered stage file: docs/stages/{name}")
    for stage, name in expected.items():
        path = root / "docs/stages" / name
        if path not in documents:
            errors.append(f"missing stage document: {path.relative_to(root)}")
        else:
            errors.extend(f"{path.relative_to(root)}: {e}" for e in stage_errors(stage, documents[path]))
    roadmap = documents.get(root / "docs/roadmap.md", "")
    errors.extend(f"docs/roadmap.md: {e}" for e in roadmap_errors(roadmap, expected))
    names = set()
    skills = [path for path in raw if path.name == "SKILL.md" and path.is_relative_to(root / ".agents/skills")]
    for path in skills:
        name, problems = skill_metadata(raw[path], path.parent.name)
        errors.extend(f"{path.relative_to(root)}: {e}" for e in problems)
        if name in names:
            errors.append(f"duplicate skill name: {name}")
        names.add(name)
    errors.extend(archive_errors(root))
    return errors, len(documents), len(skills)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    errors, count, skills = check_project(args.root)
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        print(f"FAIL: {len(errors)} issue(s); checked {count} Markdown files and {skills} skills.")
        return 1
    print(f"PASS: {count} Markdown files, {len(STAGE_FILES)} stages, {skills} skills; archive intact.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
