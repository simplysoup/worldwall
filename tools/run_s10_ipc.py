#!/usr/bin/env python3
"""Exercise the running S10 control socket against the assigned Wayland host.

This is an integration check, not a mock or headless acceptance test. It starts
one Worldwall process on the current Wayland session, uses a private runtime
directory, and records bounded-client, stale-path, peer-permission, generation,
configuration-transition, and diagnostic-rate-limit observations.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import stat
import subprocess
import tempfile
import threading
import time
from typing import Any

MAX_REQUEST = 4 * 1024
MAX_RESPONSE = 16 * 1024
ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "fixtures" / "untracked" / "effectgames"


def frame(value: dict[str, Any]) -> bytes:
    body = json.dumps(value, separators=(",", ":")).encode()
    if len(body) > MAX_REQUEST:
        raise AssertionError("test request exceeded protocol limit")
    return len(body).to_bytes(4, "big") + body


def response(stream: socket.socket) -> dict[str, Any]:
    length = int.from_bytes(read_exact(stream, 4), "big")
    if not 0 < length <= MAX_RESPONSE:
        raise AssertionError(f"invalid response length {length}")
    return json.loads(read_exact(stream, length))


def read_exact(stream: socket.socket, length: int) -> bytes:
    result = bytearray()
    while len(result) < length:
        chunk = stream.recv(length - len(result))
        if not chunk:
            raise AssertionError("socket closed before a complete response")
        result.extend(chunk)
    return bytes(result)


def request(path: Path, value: dict[str, Any], timeout: float = 3.0) -> dict[str, Any]:
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
        stream.settimeout(timeout)
        stream.connect(str(path))
        stream.sendall(frame(value))
        return response(stream)


def command(name: str, **extra: Any) -> dict[str, Any]:
    return {"version": 1, "command": name, **extra}


def expect_ok(result: dict[str, Any], label: str) -> dict[str, Any]:
    if result.get("ok") is not True:
        raise AssertionError(f"{label} failed: {result}")
    return result


def copy_registry(destination: Path, packages: list[str]) -> None:
    destination.mkdir()
    for package in packages:
        shutil.copytree(REGISTRY / package, destination / package)
    fallback = packages[0]
    (destination / "registry.toml").write_text(
        "schema_version = 1\n"
        f'fallback = "{fallback}"\n'
        "scenes = ["
        + ", ".join(json.dumps(package) for package in packages)
        + "]\n",
        encoding="utf-8",
    )


def chown_tree(path: Path, uid: int, gid: int) -> None:
    os.chown(path, uid, gid)
    if path.is_dir():
        for child in path.rglob("*"):
            os.chown(child, uid, gid)


def write_config(path: Path, registry: Path | None, fps: int) -> None:
    registry_value = "none" if registry is None else str(registry)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(
        f'schema_version = 1\nfps = {fps}\nregistry_dir = "{registry_value}"\n',
        encoding="utf-8",
    )
    os.replace(temporary, path)


def wait_for_socket(
    path: Path,
    process: subprocess.Popen[str],
    running_as_root: bool,
    timeout: float = 12.0,
) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            if running_as_root:
                # Root is intentionally an unauthorized peer until the
                # explicit credential probe below. Mode 0600 also proves that
                # the service replaced the pre-bound stale socket.
                if stat.S_IMODE(path.stat().st_mode) == 0o600:
                    return
            else:
                try:
                    expect_ok(request(path, command("status"), timeout=0.25), "startup status")
                    return
                except (OSError, AssertionError, socket.timeout):
                    pass
        if process.poll() is not None:
            raise AssertionError(f"Worldwall exited during startup with {process.returncode}")
        time.sleep(0.05)
    raise AssertionError("Worldwall control socket did not appear")


def run(args: argparse.Namespace) -> Path:
    if not REGISTRY.is_dir():
        raise SystemExit(f"required local fixture registry is unavailable: {REGISTRY}")
    output = Path(args.output_dir).expanduser().resolve()
    output.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="worldwall-s10-live-") as temporary:
        root = Path(temporary)
        runtime = root / "runtime"
        runtime.mkdir(mode=0o755)
        service_uid = os.getuid()
        service_gid = os.getgid()
        running_as_root = os.geteuid() == 0
        if running_as_root:
            service_uid = int(os.environ.get("SUDO_UID", os.stat("/run/user").st_uid))
            service_gid = int(os.environ.get("SUDO_GID", os.stat("/run/user").st_gid))
            os.chown(output, service_uid, service_gid)
            # The application deliberately requires a user-owned runtime
            # directory. When this runner is invoked with sudo, run the
            # application as the original desktop user instead of root.
            root.chmod(0o755)
            os.chown(root, service_uid, service_gid)
            os.chown(runtime, service_uid, service_gid)
        wayland_display = os.environ.get("WAYLAND_DISPLAY", "wayland-1")
        real_runtime = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
        wayland_socket = Path(wayland_display)
        if not wayland_socket.is_absolute():
            wayland_socket = real_runtime / wayland_socket
            (runtime / wayland_display).symlink_to(wayland_socket)
        socket_path = runtime / "worldwall.sock"
        stale = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        stale.bind(str(socket_path))
        stale.close()
        if running_as_root:
            os.chown(socket_path, service_uid, service_gid)

        registry_a = root / "registry-a"
        registry_b = root / "registry-b"
        copy_registry(registry_a, ["v26-winter-forest", "v29-seascape"])
        copy_registry(registry_b, ["v29-seascape", "v29-seascape--cloudy"])
        config = root / "worldwall.conf"
        write_config(config, registry_a, 15)
        if running_as_root:
            chown_tree(registry_a, service_uid, service_gid)
            chown_tree(registry_b, service_uid, service_gid)
            os.chown(config, service_uid, service_gid)
        log_path = output / "worldwall.log"
        log_file = log_path.open("w", encoding="utf-8")
        environment = os.environ.copy()
        environment.update(
            {
                "XDG_RUNTIME_DIR": str(runtime),
                "WORLDWALL_CONFIG": str(config),
                "WORLDWALL_TEST_EXIT_AFTER_MS": "30000",
                "WORLDWALL_REGISTRY_DIR": "",
            }
        )
        service_command = [args.binary]
        if running_as_root:
            service_command = [
                "setpriv",
                "--reuid",
                str(service_uid),
                "--regid",
                str(service_gid),
                "--init-groups",
                "--",
                args.binary,
            ]
        process = subprocess.Popen(
            service_command,
            cwd=ROOT,
            env=environment,
            stdout=log_file,
            stderr=subprocess.STDOUT,
            text=True,
        )
        stalled: list[socket.socket] = []
        results: dict[str, Any] = {"socket": str(socket_path), "checks": {}}
        try:
            wait_for_socket(socket_path, process, running_as_root)
            mode = stat.S_IMODE(socket_path.stat().st_mode)
            if mode != 0o600:
                raise AssertionError(f"control socket mode is {mode:o}, expected 600")
            results["checks"]["stale_path_and_permissions"] = "pass"

            unauthorized_checked = False
            if running_as_root:
                # Temporarily make the socket reachable so the server-side
                # SO_PEERCRED rejection is exercised rather than only the
                # kernel's 0600 filesystem check. Root is the foreign peer for
                # this probe; afterward all ordinary clients run as the
                # original desktop user.
                os.chmod(socket_path, 0o666)
                try:
                    probe = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                    probe.settimeout(3)
                    probe.connect(str(socket_path))
                    try:
                        probe.sendall(frame(command("status")))
                        data = probe.recv(1)
                    except (BrokenPipeError, ConnectionResetError):
                        data = b""
                    finally:
                        probe.close()
                    if data:
                        raise AssertionError("foreign root peer received a response")
                finally:
                    if socket_path.exists():
                        os.chmod(socket_path, 0o600)
                results["checks"]["unauthorized_peer"] = "pass"
                os.setgroups([service_gid])
                os.setgid(service_gid)
                os.setuid(service_uid)
                unauthorized_checked = True

            if not unauthorized_checked:
                unauthorized = subprocess.run(
                    [
                        "runuser",
                        "-u",
                        "nobody",
                        "--",
                        os.sys.executable,
                        "-c",
                        (
                            "import socket,sys; s=socket.socket(socket.AF_UNIX); "
                            "\ntry: s.connect(sys.argv[1])\nexcept PermissionError: sys.exit(0)\n"
                            "except OSError: sys.exit(0)\n"
                            "body=b'{\\\"version\\\":1,\\\"command\\\":\\\"status\\\"}'\n"
                            "s.sendall(len(body).to_bytes(4,'big')+body)\n"
                            "try: data=s.recv(1)\nexcept (BrokenPipeError,ConnectionResetError): data=b''\n"
                            "sys.exit(0 if not data else 1)"
                        ),
                        str(socket_path),
                    ],
                    capture_output=True,
                    text=True,
                    check=False,
                )
                if unauthorized.returncode == 0:
                    results["checks"]["unauthorized_peer"] = "pass"
                elif "may not be used by non-root users" in unauthorized.stderr:
                    results["checks"]["unauthorized_peer"] = {
                        "outcome": "not-run",
                        "reason": "runuser requires root on this host",
                    }
                else:
                    raise AssertionError(
                        "unauthorized user unexpectedly reached the control socket: "
                        + unauthorized.stderr
                    )

            malformed = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            malformed.settimeout(3)
            malformed.connect(str(socket_path))
            body = b'{"version":99,"command":"status"}'
            malformed.sendall(len(body).to_bytes(4, "big") + body)
            malformed_result = response(malformed)
            malformed.close()
            if malformed_result.get("ok") is not False:
                raise AssertionError(f"unsupported version was accepted: {malformed_result}")
            results["checks"]["unsupported_version_live"] = malformed_result

            # Half of the bounded client budget remains free while these
            # clients wait for their two-second read deadline.
            for _ in range(8):
                stream = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                stream.settimeout(4)
                stream.connect(str(socket_path))
                stalled.append(stream)
            started = time.monotonic()
            expect_ok(request(socket_path, command("status")), "status during stalled clients")
            elapsed = time.monotonic() - started
            if elapsed > 1.0:
                raise AssertionError(f"status was delayed by stalled clients: {elapsed:.3f}s")
            results["checks"]["stalled_clients"] = {"pass": True, "status_seconds": elapsed}
            for stream in stalled:
                stream.close()
            stalled.clear()

            def concurrent_status(_: int) -> dict[str, Any]:
                return request(socket_path, command("status"))

            threads: list[threading.Thread] = []
            concurrent_results: list[dict[str, Any]] = []
            lock = threading.Lock()

            def worker(index: int) -> None:
                value = concurrent_status(index)
                with lock:
                    concurrent_results.append(value)

            for index in range(12):
                thread = threading.Thread(target=worker, args=(index,))
                thread.start()
                threads.append(thread)
            for thread in threads:
                thread.join()
            if len(concurrent_results) != 12 or not all(
                result.get("ok") is True for result in concurrent_results
            ):
                raise AssertionError(f"concurrent status results were not all successful: {concurrent_results}")
            results["checks"]["concurrent_clients"] = {"pass": True, "count": len(concurrent_results)}

            initial = expect_ok(request(socket_path, command("status")), "initial status")
            initial_data = initial["data"]
            if initial_data["fps"] != 15:
                raise AssertionError(f"initial config was not applied: {initial_data}")
            initial_scene = initial_data["scene"]
            initial_generation = initial_data["selection_generation"]

            write_config(config, registry_b, 30)
            changed = expect_ok(request(socket_path, command("reload-config")), "registry path transition")
            if changed["data"].get("reselected") is not False:
                raise AssertionError(f"config reload unexpectedly reselected: {changed}")
            after_path = expect_ok(request(socket_path, command("status")), "status after registry transition")["data"]
            if after_path["scene"] != initial_scene or after_path["selection_generation"] != initial_generation:
                raise AssertionError(f"registry transition changed resident selection: {after_path}")
            results["checks"]["registry_path_transition"] = {"pass": True, "status": after_path}

            # Exercise rapid explicit commands. They are serialized by the
            # event-loop owner; every accepted command must have a response.
            stress_commands = [
                command("scene", id="v29-seascape"),
                command("scene", id="v29-seascape--cloudy"),
            ] * 6
            stress_results = [request(socket_path, value) for value in stress_commands]
            if not all(result.get("ok") is True for result in stress_results):
                raise AssertionError(f"late-generation stress had failures: {stress_results}")
            stressed = expect_ok(request(socket_path, command("status")), "stress status")["data"]
            if stressed["selection_generation"] < initial_generation + len(stress_commands):
                raise AssertionError(f"selection generation did not advance for stress: {stressed}")
            results["checks"]["late_generation_stress"] = {
                "pass": True,
                "commands": len(stress_commands),
                "status": stressed,
            }

            write_config(config, None, 30)
            disabled = expect_ok(request(socket_path, command("reload-config")), "disable registry")
            retained = expect_ok(request(socket_path, command("status")), "status after disabling registry" )["data"]
            if retained["scene"] != stressed["scene"]:
                raise AssertionError(f"disabling registry discarded resident scene: {retained}")
            unavailable = request(socket_path, command("scene", id="v29-seascape"))
            if unavailable.get("error", {}).get("code") != "selection_unavailable":
                raise AssertionError(f"disabled registry remained selectable: {unavailable}")
            results["checks"]["registry_disable_retains_scene"] = {
                "pass": True,
                "response": disabled,
                "status": retained,
            }

            write_config(config, registry_a, 15)
            expect_ok(request(socket_path, command("reload-config")), "re-enable registry")
            missing = command("scene", id="not-registered")
            for _ in range(12):
                result = request(socket_path, missing)
                if result.get("error", {}).get("code") not in {"selection_rejected", "selection_failed"}:
                    raise AssertionError(f"missing scene had unexpected response: {result}")
            time.sleep(1.15)
            request(socket_path, missing)
            results["checks"]["diagnostic_rate_limit"] = "pass"

            malformed_oversized = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            malformed_oversized.settimeout(3)
            malformed_oversized.connect(str(socket_path))
            malformed_oversized.sendall((MAX_REQUEST + 1).to_bytes(4, "big"))
            oversized_result = response(malformed_oversized)
            malformed_oversized.close()
            if oversized_result.get("ok") is not False:
                raise AssertionError("oversized request was accepted")
            results["checks"]["oversized_request_live"] = oversized_result
        finally:
            for stream in stalled:
                stream.close()
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
            log_file.close()

        log_text = log_path.read_text(encoding="utf-8")
        diagnostic_lines = [line for line in log_text.splitlines() if "trace ipc-diagnostic" in line]
        if len(diagnostic_lines) < 2 or not any("suppressed=" in line and "suppressed=0" not in line for line in diagnostic_lines):
            raise AssertionError(
                "repeated IPC failures did not produce a rate-limited diagnostic pair: "
                + repr(diagnostic_lines)
            )
        results["diagnostic_lines"] = diagnostic_lines
        results["log_sha256"] = __import__("hashlib").sha256(log_text.encode()).hexdigest()
        result_path = output / "results.json"
        result_path.write_text(json.dumps(results, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        return result_path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default=str(ROOT / "target" / "debug" / "worldwall"))
    parser.add_argument(
        "--output-dir",
        default=str(Path.home() / ".local" / "state" / "worldwall" / "s10-ipc-live"),
    )
    args = parser.parse_args()
    result = run(args)
    print(f"S10 live IPC checks passed: {result}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
