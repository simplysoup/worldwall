#!/usr/bin/env python3
"""Build a small, identifiable Worldwall release-candidate package.

This tool packages binaries and operator documentation only. It deliberately
never copies scene fixtures or artwork into the package.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUTPUT = ROOT / "target" / "worldwall-release"
PACKAGE_FILES = (
    ("README.md", "README.md"),
    ("LICENSE", "LICENSE"),
    ("docs/delivery-contract.md", "docs/delivery-contract.md"),
    ("docs/ipc.md", "docs/ipc.md"),
    ("docs/release.md", "docs/release.md"),
    ("docs/scene-format.md", "docs/scene-format.md"),
    ("docs/validation.md", "docs/validation.md"),
    ("examples/worldwall.toml", "examples/worldwall.toml"),
)


class PackageError(RuntimeError):
    """A release package cannot be produced safely."""


def run(command: list[str], *, cwd: Path = ROOT) -> str:
    try:
        completed = subprocess.run(
            command,
            cwd=cwd,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        output = getattr(error, "stdout", "") or ""
        detail = f"\n{output.strip()}" if output.strip() else ""
        raise PackageError(f"command failed: {' '.join(command)}{detail}") from error
    return completed.stdout.strip()


def cargo_version() -> str:
    cargo = ROOT / "Cargo.toml"
    text = cargo.read_text(encoding="utf-8")
    match = re.search(r"^version\s*=\s*\"([^\"]+)\"\s*$", text, re.MULTILINE)
    if match is None:
        raise PackageError("Cargo.toml has no package version")
    return match.group(1)


def source_identity() -> tuple[str, bool]:
    revision = run(["git", "rev-parse", "HEAD"])
    dirty = bool(run(["git", "status", "--porcelain", "--untracked-files=all"]))
    return revision, dirty


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def require_regular_file(path: Path, description: str) -> None:
    if path.is_symlink() or not path.is_file():
        raise PackageError(f"{description} is not a regular file: {path}")


def copy_file(source: Path, destination: Path) -> None:
    require_regular_file(source, "package input")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)


def package_inputs(release_dir: Path) -> Iterable[tuple[Path, Path]]:
    yield ROOT / "target" / "release" / "worldwall", release_dir / "bin" / "worldwall"
    yield ROOT / "target" / "release" / "worldwallctl", release_dir / "bin" / "worldwallctl"
    for source, relative in PACKAGE_FILES:
        yield ROOT / source, release_dir / relative


def reject_artwork_or_fixture_paths(root: Path) -> None:
    forbidden_parts = {"fixtures", "assets", "artwork"}
    forbidden_suffixes = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"}
    for path in root.rglob("*"):
        if not path.is_file():
            continue
        relative = path.relative_to(root)
        if forbidden_parts.intersection(relative.parts) or path.suffix.lower() in forbidden_suffixes:
            raise PackageError(f"release package contains forbidden artwork/fixture path: {relative}")


def file_entries(root: Path) -> list[dict[str, object]]:
    entries: list[dict[str, object]] = []
    for path in sorted((path for path in root.rglob("*") if path.is_file())):
        relative = path.relative_to(root).as_posix()
        entries.append({"path": relative, "bytes": path.stat().st_size, "sha256": sha256(path)})
    return entries


def artifact_digest(entries: list[dict[str, object]]) -> str:
    digest = hashlib.sha256()
    for entry in entries:
        digest.update(
            f"{entry['path']}\0{entry['bytes']}\0{entry['sha256']}\n".encode("utf-8")
        )
    return digest.hexdigest()


def make_package(output: Path, *, skip_build: bool, allow_dirty: bool, force: bool) -> Path:
    revision, dirty = source_identity()
    if dirty and not allow_dirty:
        raise PackageError("working tree is dirty; use --allow-dirty only for a local smoke package")
    if not skip_build:
        run(["cargo", "build", "--locked", "--release"])

    output = output.expanduser().resolve()
    if output == ROOT or ROOT not in output.parents:
        raise PackageError("output directory must be inside the repository")
    if output.exists():
        if not force:
            raise PackageError(f"output directory exists; use --force to replace it: {output}")
        if output.is_symlink() or not output.is_dir():
            raise PackageError(f"refusing to replace non-directory output: {output}")
        shutil.rmtree(output)
    output.mkdir(parents=True)
    for source, destination in package_inputs(output):
        copy_file(source, destination)
    reject_artwork_or_fixture_paths(output)

    entries = file_entries(output)
    manifest = {
        "schema_version": 1,
        "package": "worldwall",
        "version": cargo_version(),
        "source_revision": revision,
        "source_dirty": dirty,
        "build_profile": "release",
        "bundled_artwork": [],
        "files": entries,
        "artifact_sha256": artifact_digest(entries),
    }
    manifest_path = output / "manifest.json"
    manifest_path.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return manifest_path


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT)
    result.add_argument("--skip-build", action="store_true", help="package an existing release build")
    result.add_argument(
        "--allow-dirty",
        action="store_true",
        help="allow local uncommitted changes; the manifest records source_dirty=true",
    )
    result.add_argument("--force", action="store_true", help="replace the selected package directory")
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        manifest = make_package(
            args.output_dir,
            skip_build=args.skip_build,
            allow_dirty=args.allow_dirty,
            force=args.force,
        )
    except PackageError as error:
        print(f"package_release: {error}", file=sys.stderr)
        return 1
    print(f"package_release: {manifest}")
    print(f"package_release: artifact_sha256={json.loads(manifest.read_text())['artifact_sha256']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
