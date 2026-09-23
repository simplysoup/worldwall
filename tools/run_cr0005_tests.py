#!/usr/bin/env python3
"""Run the offline CR-0005 test batch and optionally prepare a private live fixture."""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SCENE = Path.home() / ".local/share/worldwall/v29-seascape-halved"
FALLBACK_SCENE = ROOT / "fixtures/untracked/effectgames/v29-seascape"
PRIVATE_UMASK = 0o077


def private_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    path.chmod(0o700)


def private_write(path: Path, data: str | bytes) -> None:
    if isinstance(data, bytes):
        path.write_bytes(data)
    else:
        path.write_text(data, encoding="utf-8")
    path.chmod(0o600)


def _contained_relative(value: Any, field: str) -> Path:
    if not isinstance(value, str):
        raise ValueError(f"scene manifest {field} must be a relative path")
    path = PurePosixPath(value)
    if path.is_absolute() or not path.parts or ".." in path.parts or "." in path.parts:
        raise ValueError(f"scene manifest {field} must stay inside the package")
    return Path(*path.parts)


def _copy_private_scene(source: Path, destination: Path) -> None:
    resolved = source.resolve(strict=True)
    if not resolved.is_dir():
        raise ValueError("scene source is not a directory")
    for item in resolved.rglob("*"):
        if item.is_symlink():
            raise ValueError("scene source contains a symbolic link")
    shutil.copytree(resolved, destination)
    for item in destination.rglob("*"):
        if item.is_dir():
            item.chmod(0o700)
        else:
            item.chmod(0o600)
    destination.chmod(0o700)


def parse_known_toml_scalars(text: str) -> dict[tuple[str | None, str], Any]:
    """Read only simple scalar assignments needed by the fixture; Rust validates the TOML."""
    values: dict[tuple[str | None, str], Any] = {}
    section: str | None = None
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith("[") and stripped.endswith("]"):
            section = stripped.strip("[]")
            continue
        if "=" not in stripped:
            continue
        key, raw = (part.strip() for part in stripped.split("=", 1))
        raw = raw.split("#", 1)[0].strip()
        if raw.startswith('"') and raw.endswith('"'):
            value: Any = json.loads(raw)
        elif raw.startswith("'") and raw.endswith("'"):
            value = raw[1:-1]
        else:
            try:
                value = float(raw)
            except ValueError:
                value = raw
        values[(section, key)] = value
    return values


def prepare_live_fixture(
    bundle_dir: Path, config_path: Path, scene_source: Path | None = None
) -> dict[str, str]:
    """Copy the source civil-time scene unchanged into a private, weather-disabled fixture."""
    source_config = config_path.expanduser().resolve(strict=True)
    try:
        parsed_config = parse_known_toml_scalars(source_config.read_text(encoding="utf-8"))
        latitude = float(parsed_config[("location", "latitude")])
        longitude = float(parsed_config[("location", "longitude")])
        timezone_name = parsed_config[("location", "timezone")]
        if (
            not math.isfinite(latitude)
            or not -90.0 <= latitude <= 90.0
            or not math.isfinite(longitude)
            or not -180.0 <= longitude <= 180.0
            or not isinstance(timezone_name, str)
            or not timezone_name
        ):
            raise ValueError
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        raise ValueError(
            "the selected private config must contain an explicit valid location and IANA timezone"
        ) from error

    if scene_source is None:
        scene_source = DEFAULT_SCENE if DEFAULT_SCENE.is_dir() else FALLBACK_SCENE
    scene_source = scene_source.expanduser().resolve(strict=True)
    if not (scene_source / "scene.toml").is_file():
        raise ValueError("scene source has no scene.toml package manifest")

    bundle_dir = bundle_dir.resolve()
    private_dir(bundle_dir)
    scene_dir = bundle_dir / "effectgames-time-of-day-scene"
    if scene_dir.exists():
        raise FileExistsError("private live fixture already exists in this bundle")
    _copy_private_scene(scene_source, scene_dir)

    manifest_path = scene_dir / "scene.toml"
    try:
        package = parse_known_toml_scalars(manifest_path.read_text(encoding="utf-8"))
        timeline_path = _contained_relative(package[(None, "time_of_day")], "time_of_day")
        timeline_file = scene_dir / timeline_path
        source_timeline = scene_source / timeline_path
        timeline = parse_known_toml_scalars(timeline_file.read_text(encoding="utf-8"))
        if timeline.get((None, "schema_version")) != 1.0:
            raise ValueError("live V29 fixture must keep its source civil-time timeline")
        atlas_path = _contained_relative(timeline[(None, "atlas")], "atlas")
        atlas_file = scene_dir / atlas_path
        source_atlas = scene_source / atlas_path
        if not atlas_file.is_file() or not source_atlas.is_file():
            raise ValueError("source time-of-day atlas is missing")
        timeline_bytes = timeline_file.read_bytes()
        atlas_bytes = atlas_file.read_bytes()
        if timeline_bytes != source_timeline.read_bytes() or atlas_bytes != source_atlas.read_bytes():
            raise ValueError("private time-of-day resources differ from the source package")
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        raise ValueError("scene source must provide an unchanged civil-time timeline and atlas") from error

    config_text = (
        'schema_version = 2\n'
        'fps = 30\n'
        'registry_dir = "none"\n\n'
        '[location]\n'
        f"latitude = {latitude!r}\n"
        f"longitude = {longitude!r}\n"
        f"timezone = {json.dumps(timezone_name)}\n\n"
        '[weather]\nprovider = "disabled"\n'
    )
    private_write(bundle_dir / "test-config.toml", config_text)
    return {
        "scene_dir": str(scene_dir),
        "config_path": str(bundle_dir / "test-config.toml"),
        "weather": "disabled",
        "timeline_schema_version": 1,
        "time_of_day_timeline_sha256": hashlib.sha256(timeline_bytes).hexdigest(),
        "time_of_day_atlas_sha256": hashlib.sha256(atlas_bytes).hexdigest(),
        "copied_scene_manifest_sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
    }


def run_command(
    name: str,
    command: list[str],
    output_dir: Path,
    environment: dict[str, str],
    timeout_seconds: int = 1800,
) -> dict[str, Any]:
    log_path = output_dir / f"{name}.log"
    started = datetime.now(timezone.utc).isoformat()
    result: dict[str, Any] = {
        "name": name,
        "command": command,
        "log": log_path.name,
        "started_utc": started,
    }
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=timeout_seconds,
            check=False,
        )
        private_write(log_path, completed.stdout)
        result["returncode"] = completed.returncode
        result["passed"] = completed.returncode == 0
    except subprocess.TimeoutExpired as error:
        text = error.stdout or ""
        if isinstance(text, bytes):
            text = text.decode("utf-8", errors="replace")
        private_write(log_path, text + f"\nTIMEOUT after {timeout_seconds} seconds\n")
        result.update({"returncode": None, "passed": False, "timeout_seconds": timeout_seconds})
    except OSError as error:
        private_write(log_path, f"could not execute command: {error}\n")
        result.update({"returncode": None, "passed": False, "execution_error": str(error)})
    result["finished_utc"] = datetime.now(timezone.utc).isoformat()
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, help="private evidence directory")
    parser.add_argument("--prepare-live", action="store_true", help="prepare but do not launch a native time-of-day Wayland fixture")
    parser.add_argument("--config", type=Path, default=Path.home() / ".config/worldwall/worldwall.toml")
    parser.add_argument("--scene-source", type=Path, help="local package to copy for the live fixture")
    args = parser.parse_args()

    os.umask(PRIVATE_UMASK)
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    output_dir = args.output_dir or (Path.home() / ".local/state/worldwall" / f"cr0005-test-batch-{stamp}")
    if output_dir.exists():
        print("evidence output directory already exists; refusing to overwrite", file=sys.stderr)
        return 2
    private_dir(output_dir)

    environment = os.environ.copy()
    for name in tuple(environment):
        if name.startswith("WORLDWALL_"):
            environment.pop(name)
    environment.update({"CARGO_TERM_COLOR": "never", "NO_COLOR": "1", "RUST_BACKTRACE": "1"})

    commands = [
        ("cargo-fmt", ["cargo", "fmt", "--check"]),
        ("cargo-test", ["cargo", "test", "--locked", "--offline", "--all-features"]),
        (
            "gpu-solar-readback",
            [
                "cargo",
                "test",
                "--locked",
                "--offline",
                "--all-features",
                "renderer::tests::gpu_time_of_day_palette_readback_matches_byte_floor_reference",
                "--",
                "--ignored",
                "--exact",
            ],
        ),
        ("cargo-clippy", ["cargo", "clippy", "--locked", "--offline", "--all-targets", "--all-features", "--", "-D", "warnings"]),
        ("cargo-build-debug-binaries", ["cargo", "build", "--locked", "--offline", "--bins"]),
        ("docs-check", ["python3", "tools/check_docs.py"]),
        ("tool-tests", ["python3", "-m", "unittest", "discover", "-s", "tools/tests", "-v"]),
        ("git-diff-check", ["git", "diff", "--check"]),
    ]
    results = [run_command(name, command, output_dir, environment) for name, command in commands]

    fixture: dict[str, Any] | None = None
    if args.prepare_live:
        try:
            fixture = prepare_live_fixture(output_dir, args.config, args.scene_source)
            fixture["prepared"] = True
        except (OSError, ValueError, FileExistsError) as error:
            fixture = {"prepared": False, "error": str(error)}
            private_write(output_dir / "live-fixture-prepare.log", f"fixture preparation failed: {error}\n")

    manifest = {
        "suite": "CR-0005 offline and assigned-session test batch",
        "started_utc": stamp,
        "workspace": str(ROOT),
        "python": sys.version.split()[0],
        "tests": results,
        "live_fixture": fixture,
        "live_run": "not-run; requires a separate explicit user-approved command and an idle Worldwall session",
    }
    private_write(output_dir / "manifest.json", json.dumps(manifest, indent=2, sort_keys=True) + "\n")

    failed = [result["name"] for result in results if not result["passed"]]
    print(f"CR-0005 offline batch: {'PASS' if not failed else 'FAIL'} ({len(results)} logged commands)")
    print(f"Evidence directory: {output_dir}")
    if fixture and fixture.get("prepared"):
        print("Private source time-of-day live fixture prepared; no Worldwall process was launched.")
        print(f"Next guide: {ROOT / 'docs/updates/2026-09-23-cr0005-test-batch-run-guide.md'}")
    elif fixture:
        print("Live fixture preparation failed; see private live-fixture-prepare.log.")
    if failed:
        print("Failed checks: " + ", ".join(failed))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
