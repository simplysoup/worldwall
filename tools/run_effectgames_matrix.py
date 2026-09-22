#!/usr/bin/env python3
"""Run the local Effect Games scene matrix sequentially on one Wayland output.

The runner deliberately starts one Worldwall process per package.  This keeps
scene uploads, startup, animation frames, and teardown attributable to one
scene and avoids competing processes on the same compositor output.
"""

from __future__ import annotations

import argparse
import ast
import csv
import datetime as dt
import json
import os
import re
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

DEFAULT_REGISTRY_DIR = Path("fixtures/untracked/effectgames")
DEFAULT_DURATION_SECONDS = 3.0
DEFAULT_TIMEOUT_GRACE_SECONDS = 10.0


def parse_registry_scene_ids(registry_path: Path) -> list[str]:
    """Read the registry's string-list scenes field without third-party TOML."""
    text = registry_path.read_text(encoding="utf-8")
    match = re.search(r"(?ms)^\s*scenes\s*=\s*(\[.*?\])\s*$", text)
    if not match:
        raise ValueError(f"registry has no scenes list: {registry_path}")
    try:
        values = ast.literal_eval(match.group(1))
    except (SyntaxError, ValueError) as exc:
        raise ValueError(f"cannot parse scenes list in {registry_path}: {exc}") from exc
    if not isinstance(values, list) or not all(isinstance(value, str) for value in values):
        raise ValueError(f"registry scenes must be a list of strings: {registry_path}")
    if len(set(values)) != len(values):
        raise ValueError(f"registry contains duplicate scene IDs: {registry_path}")
    return values


def parse_key_value(value: str) -> tuple[str, str]:
    if "=" not in value:
        raise argparse.ArgumentTypeError("expected NAME=VALUE")
    name, env_value = value.split("=", 1)
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
        raise argparse.ArgumentTypeError(f"invalid environment variable name: {name!r}")
    return name, env_value


def load_config(path: Path | None) -> dict[str, Any]:
    if path is None:
        return {}
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"cannot read JSON config {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise ValueError("matrix config must be a JSON object")
    return value


def config_value(config: dict[str, Any], key: str, cli_value: Any, default: Any) -> Any:
    return cli_value if cli_value is not None else config.get(key, default)


def resolve_scenes(
    config: dict[str, Any],
    cli_scenes: list[str] | None,
    registry_dir: Path,
) -> list[str]:
    if cli_scenes:
        scenes = cli_scenes
    elif "scenes" in config:
        scenes = config["scenes"]
        if not isinstance(scenes, list) or not all(isinstance(scene, str) for scene in scenes):
            raise ValueError("config 'scenes' must be a list of strings")
    else:
        scenes = parse_registry_scene_ids(registry_dir / "registry.toml")
    if not scenes:
        raise ValueError("scene matrix is empty")
    if len(set(scenes)) != len(scenes):
        raise ValueError("scene matrix contains duplicate IDs")
    return scenes


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, help="JSON config file; CLI values override it")
    parser.add_argument("--registry-dir", type=Path, help="Effect Games registry directory")
    parser.add_argument("--binary", type=Path, help="Worldwall executable")
    parser.add_argument("--fps", type=int, choices=(15, 30), help="render FPS")
    parser.add_argument(
        "--duration-seconds",
        type=float,
        help="debug-run duration for each scene (default: 3)",
    )
    parser.add_argument(
        "--timeout-grace-seconds",
        type=float,
        help="extra per-scene timeout after the requested duration (default: 10)",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        help="run directory for logs and summaries",
    )
    parser.add_argument(
        "--scene",
        dest="scenes",
        action="append",
        help="run only this registry scene ID; repeat to select several",
    )
    parser.add_argument(
        "--env",
        dest="environment",
        action="append",
        type=parse_key_value,
        metavar="NAME=VALUE",
        help="extra environment variable; repeat as needed",
    )
    parser.add_argument(
        "--utc-offset-minutes",
        type=int,
        help="value for WORLDWALL_UTC_OFFSET_MINUTES (default: 0)",
    )
    parser.add_argument(
        "--fail-fast",
        action="store_true",
        help="stop after the first failed or timed-out scene",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the resolved matrix without starting Worldwall",
    )
    return parser.parse_args(argv)


def default_output_dir() -> Path:
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    return Path.home() / ".local" / "state" / "worldwall" / f"effectgames-matrix-{stamp}"


def require_positive(name: str, value: float) -> None:
    if value <= 0:
        raise ValueError(f"{name} must be greater than zero")


def extract_trace_value(trace: str, key: str) -> str:
    matches = re.findall(rf"(?:^|\s){re.escape(key)}=([^\s]+)", trace)
    return matches[-1] if matches else "unavailable"


def as_text(value: str | bytes | None) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    return value


def run_scene(
    scene_id: str,
    registry_dir: Path,
    binary: Path,
    fps: int,
    duration_seconds: float,
    timeout_grace_seconds: float,
    output_dir: Path,
    utc_offset_minutes: int,
    extra_environment: dict[str, str],
) -> dict[str, Any]:
    scene_dir = registry_dir / scene_id
    scene_output = output_dir / scene_id
    scene_output.mkdir(parents=True, exist_ok=True)
    log_path = scene_output / "worldwall.log"
    manifest_path = scene_output / "run.json"
    if not scene_dir.is_dir():
        raise ValueError(f"missing scene directory for {scene_id!r}: {scene_dir}")

    duration_ms = max(1, round(duration_seconds * 1000))
    environment = os.environ.copy()
    environment.update(extra_environment)
    environment.update(
        {
            "WORLDWALL_SCENE_DIR": str(scene_dir.resolve()),
            "WORLDWALL_FPS": str(fps),
            "WORLDWALL_UTC_OFFSET_MINUTES": str(utc_offset_minutes),
            "WORLDWALL_TEST_EXIT_AFTER_MS": str(duration_ms),
        }
    )
    command = [str(binary.resolve())]
    started = time.monotonic()
    timed_out = False
    return_code: int | None = None
    output = ""
    process: subprocess.Popen[str] | None = None
    try:
        process = subprocess.Popen(
            command,
            cwd=Path.cwd(),
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            errors="replace",
            start_new_session=True,
        )
        try:
            output, _ = process.communicate(timeout=duration_seconds + timeout_grace_seconds)
        except subprocess.TimeoutExpired as exc:
            timed_out = True
            output = as_text(exc.output)
            os.killpg(process.pid, signal.SIGTERM)
            try:
                trailing, _ = process.communicate(timeout=5)
                output += as_text(trailing)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                trailing, _ = process.communicate()
                output += as_text(trailing)
        return_code = process.returncode
    finally:
        elapsed = time.monotonic() - started

    log_path.write_text(output, encoding="utf-8")
    result = {
        "scene": scene_id,
        "scene_dir": str(scene_dir.resolve()),
        "log": str(log_path),
        "returncode": return_code,
        "timed_out": timed_out,
        "elapsed_seconds": round(elapsed, 3),
        "animation_frames": output.count("trace animation-frame"),
        "scene_upload_count": extract_trace_value(output, "scene-upload-count"),
        "clean_deadline_exit": "trace test-exit clean=true reason=deadline" in output,
        "owner_safe_teardown": "trace owners-dropped gpu-before-layer-before-selected-output=true" in output,
    }
    manifest_path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


def write_summary(output_dir: Path, results: list[dict[str, Any]], resolved: dict[str, Any]) -> None:
    (output_dir / "resolved-config.json").write_text(
        json.dumps(resolved, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    fields = [
        "scene",
        "returncode",
        "timed_out",
        "elapsed_seconds",
        "animation_frames",
        "scene_upload_count",
        "clean_deadline_exit",
        "owner_safe_teardown",
        "log",
    ]
    with (output_dir / "summary.csv").open("w", encoding="utf-8", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=fields)
        writer.writeheader()
        writer.writerows({field: result.get(field, "") for field in fields} for result in results)
    (output_dir / "summary.json").write_text(
        json.dumps({"config": resolved, "results": results}, indent=2) + "\n",
        encoding="utf-8",
    )


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        config = load_config(args.config)
        registry_dir = Path(
            config_value(config, "registry_dir", args.registry_dir, DEFAULT_REGISTRY_DIR)
        ).resolve()
        binary = Path(
            config_value(config, "binary", args.binary, os.environ.get("WORLDWALL_BIN", "target/debug/worldwall"))
        ).resolve()
        fps = int(config_value(config, "fps", args.fps, 30))
        duration_seconds = float(
            config_value(config, "duration_seconds", args.duration_seconds, DEFAULT_DURATION_SECONDS)
        )
        timeout_grace_seconds = float(
            config_value(
                config,
                "timeout_grace_seconds",
                args.timeout_grace_seconds,
                DEFAULT_TIMEOUT_GRACE_SECONDS,
            )
        )
        utc_offset_minutes = int(
            config_value(config, "utc_offset_minutes", args.utc_offset_minutes, 0)
        )
        output_dir = Path(
            config_value(config, "output_dir", args.output_dir, default_output_dir())
        ).resolve()
        scenes = resolve_scenes(config, args.scenes, registry_dir)
        require_positive("duration_seconds", duration_seconds)
        require_positive("timeout_grace_seconds", timeout_grace_seconds)
        if fps not in (15, 30):
            raise ValueError("fps must be 15 or 30")
        if not registry_dir.is_dir():
            raise ValueError(f"missing registry directory: {registry_dir}")
        if not args.dry_run and not binary.is_file():
            raise ValueError(f"missing executable: {binary}; run cargo build --locked first")

        configured_environment = config.get("env", {})
        if not isinstance(configured_environment, dict):
            raise ValueError("config 'env' must be an object of string values")
        extra_environment = {
            str(name): str(value) for name, value in configured_environment.items()
        }
        extra_environment.update(dict(args.environment or []))
        resolved = {
            "registry_dir": str(registry_dir),
            "binary": str(binary),
            "fps": fps,
            "duration_seconds": duration_seconds,
            "timeout_grace_seconds": timeout_grace_seconds,
            "utc_offset_minutes": utc_offset_minutes,
            "output_dir": str(output_dir),
            "scenes": scenes,
            "extra_environment_names": sorted(extra_environment),
        }
        if args.dry_run:
            print(json.dumps(resolved, indent=2))
            return 0

        output_dir.mkdir(parents=True, exist_ok=False)
        results: list[dict[str, Any]] = []
        failures = 0
        print(f"running {len(scenes)} scenes at {fps} FPS for {duration_seconds:g}s")
        for index, scene_id in enumerate(scenes, 1):
            print(f"[{index}/{len(scenes)}] {scene_id}", flush=True)
            try:
                result = run_scene(
                    scene_id,
                    registry_dir,
                    binary,
                    fps,
                    duration_seconds,
                    timeout_grace_seconds,
                    output_dir,
                    utc_offset_minutes,
                    extra_environment,
                )
            except (OSError, ValueError) as exc:
                failures += 1
                result = {
                    "scene": scene_id,
                    "returncode": None,
                    "timed_out": False,
                    "error": str(exc),
                }
                print(f"  ERROR: {exc}", file=sys.stderr)
            else:
                failed = (
                    result["returncode"] != 0
                    or result["timed_out"]
                    or not result["clean_deadline_exit"]
                    or not result["owner_safe_teardown"]
                )
                if failed:
                    failures += 1
                    print(f"  FAIL: {result}", file=sys.stderr)
                else:
                    print(
                        f"  pass frames={result['animation_frames']} "
                        f"uploads={result['scene_upload_count']}",
                        flush=True,
                    )
            results.append(result)
            if failures and args.fail_fast:
                break

        write_summary(output_dir, results, resolved)
        print(f"summary: {output_dir / 'summary.csv'}")
        return 1 if failures else 0
    except (OSError, ValueError) as exc:
        print(f"run_effectgames_matrix: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
