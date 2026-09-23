#!/usr/bin/env python3
"""Run the guarded Effect Games civil-time palette check in an idle Wayland session."""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import re
import signal
import statistics
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, TextIO

from run_cr0005_tests import _contained_relative, parse_known_toml_scalars

ROOT = Path(__file__).resolve().parents[1]
REQUIRED_LIVE_CHECKS = (
    "process_exit_successfully",
    "deadline_exit_not_forced",
    "runner_completed_without_exception",
    "package_scene_loaded",
    "weather_disabled",
    "source_time_of_day_preserved",
    "clean_debug_exit",
    "shutdown_logged",
    "all_control_commands_succeeded_except_expected_invalid",
    "fps_reloads_not_reselected",
    "invalid_reload_rejected",
    "scene_uploads_stable_at_one",
    "test_config_restored",
)


def failed_checks(checks: dict[str, Any]) -> list[str]:
    return [name for name in REQUIRED_LIVE_CHECKS if checks.get(name) is not True]


def private_write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8")
    path.chmod(0o600)


def save_json(path: Path, value: Any) -> None:
    private_write(path, json.dumps(value, indent=2, sort_keys=True) + "\n")


def open_private_log(path: Path, **kwargs: Any) -> TextIO:
    private_write(path, "")
    return path.open(**kwargs)


def worldwall_processes() -> list[dict[str, Any]]:
    processes: list[dict[str, Any]] = []
    proc_root = Path("/proc")
    for entry in proc_root.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            executable = os.readlink(entry / "exe")
        except (OSError, PermissionError):
            continue
        if Path(executable).name.lower() == "worldwall":
            processes.append({"pid": int(entry.name), "executable_name": "worldwall"})
    return sorted(processes, key=lambda process: process["pid"])


def validate_wayland_session() -> str:
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    display = os.environ.get("WAYLAND_DISPLAY")
    if not runtime or not display:
        raise RuntimeError("WAYLAND_DISPLAY and XDG_RUNTIME_DIR are required")
    socket_path = Path(display) if Path(display).is_absolute() else Path(runtime) / display
    if not socket_path.exists() or not socket_path.is_socket():
        raise RuntimeError("the configured Wayland socket is unavailable")
    return str(socket_path)


def write_config(path: Path, fps: int, location: dict[str, Any]) -> None:
    latitude = float(location["latitude"])
    longitude = float(location["longitude"])
    zone = str(location["timezone"])
    content = (
        'schema_version = 2\n'
        f"fps = {fps}\n"
        'registry_dir = "none"\n\n'
        '[location]\n'
        f"latitude = {latitude!r}\n"
        f"longitude = {longitude!r}\n"
        f"timezone = {json.dumps(zone)}\n\n"
        '[weather]\nprovider = "disabled"\n'
    )
    temporary = path.with_suffix(".tmp")
    private_write(temporary, content)
    os.replace(temporary, path)
    path.chmod(0o600)


def parse_response(path: Path) -> dict[str, Any] | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def run_ctl(client: Path, command: str, log_path: Path, environment: dict[str, str]) -> dict[str, Any]:
    started = time.monotonic_ns()
    try:
        result = subprocess.run(
            [str(client), command],
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=5,
            check=False,
        )
        private_write(log_path, result.stdout)
        response = parse_response(log_path)
        return {
            "command": command,
            "log": log_path.name,
            "returncode": result.returncode,
            "response": response,
            "elapsed_ms": (time.monotonic_ns() - started) / 1_000_000,
        }
    except (OSError, subprocess.TimeoutExpired) as error:
        private_write(log_path, f"control command failed: {error}\n")
        return {
            "command": command,
            "log": log_path.name,
            "returncode": None,
            "response": None,
            "error": type(error).__name__,
        }


def response_result(result: dict[str, Any]) -> dict[str, Any] | None:
    response = result.get("response")
    if not isinstance(response, dict):
        return None
    body = response.get("data")
    return body if isinstance(body, dict) else None


def source_time_of_day_preserved(bundle: Path, scene_dir: Path) -> bool:
    try:
        manifest = json.loads((bundle / "manifest.json").read_text(encoding="utf-8"))
        fixture = manifest["live_fixture"]
        package = parse_known_toml_scalars((scene_dir / "scene.toml").read_text(encoding="utf-8"))
        timeline_path = scene_dir / _contained_relative(package[(None, "time_of_day")], "time_of_day")
        timeline = parse_known_toml_scalars(timeline_path.read_text(encoding="utf-8"))
        if timeline.get((None, "schema_version")) != 1.0:
            return False
        atlas_path = scene_dir / _contained_relative(timeline[(None, "atlas")], "atlas")
        timeline_hash = hashlib.sha256(timeline_path.read_bytes()).hexdigest()
        atlas_hash = hashlib.sha256(atlas_path.read_bytes()).hexdigest()
        return (
            str(scene_dir) == fixture.get("scene_dir")
            and timeline_hash == fixture.get("time_of_day_timeline_sha256")
            and atlas_hash == fixture.get("time_of_day_atlas_sha256")
        )
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError):
        return False


def frame_fields(line: str) -> dict[str, str]:
    return dict(re.findall(r"([A-Za-z_-]+)=([^\s]+)", line))


def metrics_summary(path: Path) -> dict[str, Any]:
    rows: list[dict[str, str]] = []
    if path.is_file():
        with path.open(newline="", encoding="utf-8") as stream:
            rows = list(csv.DictReader(stream))
    times = [int(row["observed_monotonic_ns"]) for row in rows]
    intervals_ms = [
        (later - earlier) / 1_000_000 for earlier, later in zip(times, times[1:])
    ]
    uploads = [
        int(row["scene_uploads"])
        for row in rows
        if row.get("scene_uploads", "").isdigit()
    ]
    updates = [
        int(row["animation_uniform_updates"])
        for row in rows
        if row.get("animation_uniform_updates", "").isdigit()
    ]
    series: dict[tuple[str, int, int], list[int]] = {}
    last_rate: dict[str, tuple[int, int]] = {}
    next_phase: dict[str, int] = {}
    for row in rows:
        output = row.get("output", "")
        fps_value = row.get("fps", "")
        time_value = row.get("observed_monotonic_ns", "")
        if not output or not fps_value.isdigit() or not time_value.isdigit():
            continue
        fps = int(fps_value)
        if fps <= 0:
            continue
        if output not in last_rate or last_rate[output][0] != fps:
            phase = next_phase.get(output, 0) + 1
            next_phase[output] = phase
            last_rate[output] = (fps, phase)
        else:
            phase = last_rate[output][1]
        series.setdefault((output, fps, phase), []).append(int(time_value))
    by_output_fps = []
    for (output, fps, phase), series_times in sorted(series.items()):
        intervals = sorted(
            (later - earlier) / 1_000_000
            for earlier, later in zip(series_times, series_times[1:])
        )
        percentile_95 = (
            intervals[min(len(intervals) - 1, int(0.95 * (len(intervals) - 1)))]
            if intervals
            else None
        )
        by_output_fps.append(
            {
                "output": output,
                "fps": fps,
                "rate_phase": phase,
                "frame_records": len(series_times),
                "interval_count": len(intervals),
                "nominal_interval_ms": 1000.0 / fps,
                "interval_median_ms": statistics.median(intervals) if intervals else None,
                "interval_p95_ms": percentile_95,
            }
        )
    intervals_sorted = sorted(intervals_ms)
    p95 = intervals_sorted[min(len(intervals_sorted) - 1, int(0.95 * (len(intervals_sorted) - 1)))] if intervals_sorted else None
    return {
        "frame_records": len(rows),
        "interval_median_ms": intervals_sorted[len(intervals_sorted) // 2] if intervals_sorted else None,
        "interval_p95_ms": p95,
        "scene_upload_values": sorted(set(uploads)),
        "animation_uniform_updates_min": min(updates) if updates else None,
        "animation_uniform_updates_max": max(updates) if updates else None,
        "by_output_fps": by_output_fps,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", required=True, type=Path, help="output directory from run_cr0005_tests.py --prepare-live")
    parser.add_argument("--confirm-live-run", action="store_true", help="explicitly authorize a bounded launch in the current Wayland session")
    parser.add_argument("--duration-seconds", type=int, default=90)
    parser.add_argument("--utc-seconds", type=int, help="optional debug-only fixed UTC timestamp for civil-time palette sampling")
    args = parser.parse_args()

    bundle = args.bundle.expanduser().resolve(strict=True)
    scene_dir = bundle / "effectgames-time-of-day-scene"
    config_path = bundle / "test-config.toml"
    result_path = bundle / "live-result.json"
    if not scene_dir.is_dir() or not config_path.is_file():
        print("bundle is missing its private Effect Games time-of-day scene or test config", file=sys.stderr)
        return 2
    bundle.chmod(0o700)
    config_path.chmod(0o600)
    processes = worldwall_processes()
    if processes:
        result = {
            "state": "blocked-active-worldwall",
            "started_utc": datetime.now(timezone.utc).isoformat(),
            "active_processes": processes,
            "launched": False,
            "required_intervention": "wait for an approved session window and stop the existing Worldwall instance yourself; this runner never stops another process",
        }
        save_json(result_path, result)
        print("Live CR-0005 test not launched: a Worldwall process is active.")
        print(f"Blocked preflight: {result_path}")
        return 3
    if not args.confirm_live_run:
        result = {
            "state": "awaiting-explicit-session-authorization",
            "started_utc": datetime.now(timezone.utc).isoformat(),
            "launched": False,
        }
        save_json(result_path, result)
        print("No process launched. Re-run with --confirm-live-run only during an approved session window.")
        return 4
    if not 60 <= args.duration_seconds <= 300:
        print("duration must be between 60 and 300 seconds", file=sys.stderr)
        return 2

    try:
        socket_path = validate_wayland_session()
        test_config = parse_known_toml_scalars(config_path.read_text(encoding="utf-8"))
        location = {
            "latitude": test_config[("location", "latitude")],
            "longitude": test_config[("location", "longitude")],
            "timezone": test_config[("location", "timezone")],
        }
        if test_config.get(("weather", "provider")) != "disabled":
            raise ValueError("live test config must keep weather disabled")
        if not source_time_of_day_preserved(bundle, scene_dir):
            raise ValueError("live fixture must preserve the source civil-time atlas")
    except (OSError, KeyError, ValueError, RuntimeError) as error:
        save_json(
            result_path,
            {"state": "preflight-failed", "launched": False, "error": type(error).__name__},
        )
        print(f"Live CR-0005 test preflight failed ({type(error).__name__}).", file=sys.stderr)
        return 2

    binary = ROOT / "target/debug/worldwall"
    client = ROOT / "target/debug/worldwallctl"
    build_log = bundle / "live-build.log"
    try:
        build = subprocess.run(
            ["cargo", "build", "--locked", "--offline", "--bins"],
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=1800,
            check=False,
        )
        private_write(build_log, build.stdout)
    except (OSError, subprocess.TimeoutExpired) as error:
        private_write(build_log, f"debug build failed: {type(error).__name__}\n")
        save_json(result_path, {"state": "build-failed", "launched": False, "log": build_log.name})
        return 1
    if build.returncode != 0 or not binary.is_file() or not client.is_file():
        save_json(result_path, {"state": "build-failed", "launched": False, "log": build_log.name})
        print(f"Debug build failed; see {build_log}", file=sys.stderr)
        return 1

    if worldwall_processes():
        save_json(
            result_path,
            {
                "state": "blocked-active-worldwall-after-build",
                "launched": False,
                "active_processes": worldwall_processes(),
            },
        )
        print("A Worldwall process started during build; live test not launched.", file=sys.stderr)
        return 3

    clock_mode = "fixed" if args.utc_seconds is not None else "system"
    environment = os.environ.copy()
    for name in tuple(environment):
        if name.startswith("WORLDWALL_"):
            environment.pop(name)
    environment.update(
        {
            "WORLDWALL_CONFIG": str(config_path),
            "WORLDWALL_SCENE_DIR": str(scene_dir),
            "WORLDWALL_TEST_EXIT_AFTER_MS": str(args.duration_seconds * 1000),
            "NO_COLOR": "1",
        }
    )
    if args.utc_seconds is not None:
        environment["WORLDWALL_TEST_UTC_SECONDS"] = str(args.utc_seconds)

    app_log = bundle / "worldwall-live.log"
    frame_path = bundle / "frame-observations.csv"
    control_results: list[dict[str, Any]] = []
    startup_probes: list[dict[str, Any]] = []
    test_started = datetime.now(timezone.utc).isoformat()
    stream_lock = threading.Lock()
    app_stream = open_private_log(app_log, mode="w", encoding="utf-8")
    frame_stream = open_private_log(frame_path, mode="w", newline="", encoding="utf-8")
    frame_writer = csv.DictWriter(
        frame_stream,
        fieldnames=[
            "observed_monotonic_ns",
            "output",
            "fps",
            "scene_uploads",
            "animation_uniform_updates",
            "phase_seconds",
        ],
    )
    frame_writer.writeheader()

    try:
        process = subprocess.Popen(
            [str(binary)],
            cwd=ROOT,
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            bufsize=1,
            start_new_session=True,
        )
    except OSError as error:
        app_stream.close()
        frame_stream.close()
        save_json(result_path, {"state": "launch-failed", "launched": False, "error": str(error)})
        return 1

    def drain_output() -> None:
        assert process.stdout is not None
        for line in process.stdout:
            observed_ns = time.monotonic_ns()
            with stream_lock:
                app_stream.write(line)
                app_stream.flush()
                if "trace animation-frame" in line:
                    fields = frame_fields(line)
                    frame_writer.writerow(
                        {
                            "observed_monotonic_ns": observed_ns,
                            "output": fields.get("output", "single"),
                            "fps": fields.get("fps", ""),
                            "scene_uploads": fields.get("scene-upload-count", fields.get("shared-scene-uploads", "")),
                            "animation_uniform_updates": fields.get("animation-uniform-updates", ""),
                            "phase_seconds": fields.get("phase-seconds", ""),
                        }
                    )
                    frame_stream.flush()

    output_thread = threading.Thread(target=drain_output, name="worldwall-log-reader", daemon=True)
    output_thread.start()
    expected_checks: dict[str, Any] = {
        "wayland_socket": socket_path,
        "source_time_of_day_preserved": True,
        "clock_mode": clock_mode,
    }
    try:
        status_before: dict[str, Any] = {}
        for attempt in range(20):
            status_before = run_ctl(
                client,
                "status",
                bundle / f"status-startup-probe-{attempt:02d}.json",
                environment,
            )
            startup_probes.append(status_before)
            if status_before.get("returncode") == 0:
                break
            time.sleep(0.5)
        control_results.append(status_before)
        baseline_status = response_result(status_before) or {}
        baseline_generation = baseline_status.get("selection_generation")
        expected_checks["status_before_fps"] = baseline_status.get("fps")

        start_monotonic = time.monotonic()
        changed_to_15 = False
        invalid_checked = False
        restored_30 = False
        next_status = start_monotonic + 5
        while process.poll() is None:
            elapsed = time.monotonic() - start_monotonic
            if elapsed >= 15 and not changed_to_15:
                write_config(config_path, 15, location)
                result = run_ctl(client, "reload-config", bundle / "reload-config-valid-15.log", environment)
                control_results.append(result)
                body = response_result(result)
                expected_checks["reload_to_15_reselected"] = body.get("reselected") if body else None
                expected_checks["reload_to_15_fps"] = body.get("fps") if body else None
                status_after_15 = run_ctl(client, "status", bundle / "status-after-15.json", environment)
                control_results.append(status_after_15)
                status_15_body = response_result(status_after_15) or {}
                expected_checks["status_after_15_fps"] = status_15_body.get("fps")
                expected_checks["status_after_15_generation_unchanged"] = (
                    status_15_body.get("selection_generation") == baseline_generation
                )
                changed_to_15 = True
            if elapsed >= 28 and changed_to_15 and not invalid_checked:
                write_config(config_path, 60, location)
                result = run_ctl(client, "reload-config", bundle / "reload-config-invalid-60.log", environment)
                control_results.append(result)
                expected_checks["invalid_reload_rejected"] = result.get("returncode") == 1
                status_after_invalid = run_ctl(
                    client, "status", bundle / "status-after-invalid.json", environment
                )
                control_results.append(status_after_invalid)
                status_invalid_body = response_result(status_after_invalid) or {}
                expected_checks["invalid_reload_kept_fps"] = status_invalid_body.get("fps") == 15
                expected_checks["invalid_reload_kept_generation"] = (
                    status_invalid_body.get("selection_generation") == baseline_generation
                )
                invalid_checked = True
            if elapsed >= 42 and invalid_checked and not restored_30:
                write_config(config_path, 30, location)
                result = run_ctl(client, "reload-config", bundle / "reload-config-valid-30.log", environment)
                control_results.append(result)
                body = response_result(result)
                expected_checks["reload_to_30_reselected"] = body.get("reselected") if body else None
                expected_checks["reload_to_30_fps"] = body.get("fps") if body else None
                status_after_30 = run_ctl(client, "status", bundle / "status-after-30.json", environment)
                control_results.append(status_after_30)
                status_30_body = response_result(status_after_30) or {}
                expected_checks["status_after_30_fps"] = status_30_body.get("fps")
                expected_checks["status_after_30_generation_unchanged"] = (
                    status_30_body.get("selection_generation") == baseline_generation
                )
                restored_30 = True
            if time.monotonic() >= next_status and process.poll() is None:
                control_results.append(
                    run_ctl(client, "status", bundle / f"status-periodic-{len(control_results):02d}.json", environment)
                )
                next_status = time.monotonic() + 15
            if process.poll() is not None:
                break
            time.sleep(0.25)
        try:
            returncode = process.wait(timeout=12)
        except subprocess.TimeoutExpired:
            process.send_signal(signal.SIGTERM)
            returncode = process.wait(timeout=5)
            expected_checks["runner_terminated_owned_child"] = True
    except KeyboardInterrupt:
        if process.poll() is None:
            process.send_signal(signal.SIGTERM)
        try:
            returncode = process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            returncode = process.wait(timeout=5)
        expected_checks["operator_interrupted"] = True
    except Exception as error:
        expected_checks["runner_completed_without_exception"] = False
        expected_checks["runner_error"] = type(error).__name__
        returncode = process.poll()
    finally:
        if process.poll() is None:
            try:
                process.send_signal(signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        output_thread.join(timeout=5)
        if process.stdout:
            process.stdout.close()
        app_stream.close()
        frame_stream.close()

    expected_checks.setdefault("runner_completed_without_exception", True)
    try:
        write_config(config_path, 30, location)
    except (OSError, KeyError, TypeError, ValueError) as error:
        expected_checks["test_config_restored"] = False
        private_write(bundle / "config-restore-error.log", f"restore failed: {type(error).__name__}\n")
    else:
        expected_checks["test_config_restored"] = True

    log_text = app_log.read_text(encoding="utf-8", errors="replace") if app_log.is_file() else ""
    frame_metrics = metrics_summary(frame_path)
    expected_checks["process_exit_successfully"] = returncode == 0
    expected_checks["deadline_exit_not_forced"] = not expected_checks.get(
        "runner_terminated_owned_child", False
    )
    expected_checks["package_scene_loaded"] = "trace scene-source=package" in log_text
    expected_checks["weather_disabled"] = "trace weather-enabled=false reason=disabled" in log_text
    expected_checks["clean_debug_exit"] = "trace test-exit clean=true reason=deadline" in log_text
    expected_checks["shutdown_logged"] = "trace shutdown state=" in log_text or "trace shutdown multi-output=true" in log_text
    expected_checks["all_control_commands_succeeded_except_expected_invalid"] = all(
        item.get("returncode") == 0
        for item in control_results
        if item["command"] != "reload-config" or item["log"] != "reload-config-invalid-60.log"
    )
    expected_checks["fps_reloads_not_reselected"] = (
        expected_checks.get("status_before_fps") == 30
        and expected_checks.get("reload_to_15_reselected") is False
        and expected_checks.get("reload_to_15_fps") == 15
        and expected_checks.get("status_after_15_fps") == 15
        and expected_checks.get("status_after_15_generation_unchanged") is True
        and expected_checks.get("invalid_reload_kept_fps") is True
        and expected_checks.get("invalid_reload_kept_generation") is True
        and expected_checks.get("reload_to_30_reselected") is False
        and expected_checks.get("reload_to_30_fps") == 30
        and expected_checks.get("status_after_30_fps") == 30
        and expected_checks.get("status_after_30_generation_unchanged") is True
    )
    expected_checks["invalid_reload_rejected"] = expected_checks.get("invalid_reload_rejected") is True
    expected_checks["scene_uploads_stable_at_one"] = frame_metrics["scene_upload_values"] == [1]

    print("Observe the copied Effect Games time-of-day palette and pacing during the run.")
    print("Record only your visual result: PASS, FAIL, or blank for NOT-RUN.")
    try:
        answer = input("Visual observation [pass/fail/not-run]: ").strip().lower()
    except (EOFError, KeyboardInterrupt):
        answer = ""
    observation = answer if answer in {"pass", "fail"} else "not-run"
    save_json(
        bundle / "visual-observation.json",
        {"result": observation, "recorded_utc": datetime.now(timezone.utc).isoformat()},
    )

    private_write(
        bundle / "frame-pacing-by-output-fps.json",
        json.dumps(frame_metrics["by_output_fps"], indent=2, sort_keys=True) + "\n",
    )
    summary = {
        "state": "completed",
        "started_utc": test_started,
        "finished_utc": datetime.now(timezone.utc).isoformat(),
        "launched": True,
        "returncode": returncode,
        "binary": str(binary),
        "logs": {
            "application": app_log.name,
            "frames": frame_path.name,
            "frame_pacing_by_output_fps": "frame-pacing-by-output-fps.json",
            "build": build_log.name,
            "control": [item["log"] for item in control_results],
            "startup_probes": [item["log"] for item in startup_probes],
            "visual_observation": "visual-observation.json",
        },
        "control_results": control_results,
        "startup_probe_results": startup_probes,
        "checks": expected_checks,
        "frame_metrics": frame_metrics,
        "visual_observation": observation,
        "clock_mode": clock_mode,
        "time_of_day_source": "schema-v1 civil timeline and atlas preserved byte-for-byte from the selected scene package",
        "network_policy": "test config disables weather; environment overrides were cleared before launch",
    }
    save_json(result_path, summary)
    failed = failed_checks(expected_checks)
    print(f"Live CR-0005 control/upload checks: {'PASS' if not failed else 'REVIEW'}")
    print("Review pacing samples per output and FPS; this runner records measurements but applies no pacing threshold.")
    print(f"Evidence directory: {bundle}")
    if failed:
        print("Checks needing review: " + ", ".join(failed))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
