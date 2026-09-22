#!/bin/sh
# Run the S08 topology-change-during-sleep subcase on a compositor using
# Hyprland's Lua configuration manager. This intentionally changes a
# non-selected output while the session is suspended, then restores its exact
# mode/layout after resume.
#
# The script is operator-facing and will not suspend the session unless the
# explicit --authorized argument is supplied.
set -eu

if [ "${1:-}" != "--authorized" ]; then
    printf '%s\n' 'Refusing to suspend the active session.' >&2
    printf '%s\n' 'Run this script with --authorized only after confirming the suspend test.' >&2
    exit 2
fi

: "${WORLDWALL_BIN:=target/debug/worldwall}"
: "${WORLDWALL_LOG:=/tmp/worldwall-s08-topology-live.log}"
: "${WORLDWALL_HELPER_LOG:=/tmp/worldwall-s08-topology-live-helper.log}"
: "${WORLDWALL_WAYLAND_DISPLAY:=${WAYLAND_DISPLAY:-wayland-1}}"
: "${WORLDWALL_OUTPUT:=HDMI-A-1}"

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
export WAYLAND_DISPLAY="$WORLDWALL_WAYLAND_DISPLAY"

if ! command -v hyprctl >/dev/null 2>&1 || ! command -v dbus-monitor >/dev/null 2>&1; then
    printf '%s\n' 'hyprctl and dbus-monitor are required.' >&2
    exit 2
fi

if ! command -v python3 >/dev/null 2>&1; then
    printf '%s\n' 'python3 is required to capture the output restore rule.' >&2
    exit 2
fi

output_json=$(hyprctl monitors -j)
restore_rule=$(OUTPUT_JSON="$output_json" OUTPUT_NAME="$WORLDWALL_OUTPUT" python3 - <<'PY'
import json
import os

name = os.environ["OUTPUT_NAME"]
monitors = json.loads(os.environ["OUTPUT_JSON"])
match = next((monitor for monitor in monitors if monitor["name"] == name), None)
if match is None:
    raise SystemExit(f"output not found: {name}")
if match["disabled"]:
    raise SystemExit(f"output is already disabled: {name}")
mode = f'{match["width"]}x{match["height"]}@{match["refreshRate"]:.2f}'
position = f'{match["x"]}x{match["y"]}'
print(f'hl.monitor({{ output = "{name}", mode = "{mode}", position = "{position}", scale = {match["scale"]}, transform = {match["transform"]}, disabled = false }})')
PY
)

output_is_enabled() {
    hyprctl monitors all -j | OUTPUT_NAME="$WORLDWALL_OUTPUT" python3 -c '
import json, os, sys
name = os.environ["OUTPUT_NAME"]
monitor = next(m for m in json.load(sys.stdin) if m["name"] == name)
sys.exit(0 if not monitor["disabled"] else 1)
'
}

restore_output() {
    # On resume Hyprland can process the Lua rule before the output backend has
    # finished re-enabling the connector. Retry the idempotent rule until the
    # compositor reports the output as enabled.
    for _ in $(seq 1 20); do
        hyprctl eval "$restore_rule" >/dev/null 2>&1 || true
        if output_is_enabled; then
            return 0
        fi
        sleep 0.25
    done
    return 1
}

stop_background() {
    pid=$1
    [ "$pid" -ne "$$" ] || return 0
    if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
        # The helper is a pipeline waiting on dbus-monitor. Do not let a
        # signal-ignoring pipeline survive the test or block cleanup forever.
        for _ in 1 2 3 4 5; do
            kill -0 "$pid" 2>/dev/null || return 0
            sleep 0.05
        done
        kill -KILL "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    fi
}

cleanup() {
    status=$?
    [ -z "${worldwall_pid:-}" ] || stop_background "$worldwall_pid"
    [ -z "${helper_pid:-}" ] || stop_background "$helper_pid"
    # A failed or interrupted cycle must not leave the operator's output
    # disabled. Retry because a suspend/resume transition can delay the
    # compositor's connector state. The rule is idempotent when already
    # restored.
    restore_output || true
    exit "$status"
}
trap cleanup EXIT INT TERM

: >"$WORLDWALL_LOG"
: >"$WORLDWALL_HELPER_LOG"

# dbus-monitor is only the trigger bridge; it never performs Wayland work.
# Hyprland 0.56.2's Lua config manager requires eval, not keyword.
(
    record_state() {
        state=$(hyprctl monitors all -j | OUTPUT_NAME="$WORLDWALL_OUTPUT" python3 -c '
import json, os, sys
name = os.environ["OUTPUT_NAME"]
monitor = next(m for m in json.load(sys.stdin) if m["name"] == name)
disabled = monitor["disabled"]
width = monitor["width"]
height = monitor["height"]
refresh = monitor["refreshRate"]
x = monitor["x"]
y = monitor["y"]
print(f"disabled={disabled} mode={width}x{height}@{refresh:.2f} position={x}x{y}")
')
        printf 'helper state output=%s %s\n' "$WORLDWALL_OUTPUT" "$state" >>"$WORLDWALL_HELPER_LOG"
    }

    dbus-monitor --system \
        "type='signal',sender='org.freedesktop.login1',interface='org.freedesktop.login1.Manager',member='PrepareForSleep'" 2>"${WORLDWALL_HELPER_LOG}.err" |
    while IFS= read -r line; do
        case "$line" in
            *"boolean true"*)
                printf 'helper event=entering-sleep action=disable output=%s\n' "$WORLDWALL_OUTPUT" >>"$WORLDWALL_HELPER_LOG"
                hyprctl eval "hl.monitor({ output = \"$WORLDWALL_OUTPUT\", disabled = true })" >>"$WORLDWALL_HELPER_LOG" 2>&1 || true
                record_state || true
                ;;
            *"boolean false"*)
                printf 'helper event=resumed action=restore output=%s\n' "$WORLDWALL_OUTPUT" >>"$WORLDWALL_HELPER_LOG"
                restore_output || true
                record_state || true
                ;;
        esac
    done
) &
helper_pid=$!

WORLDWALL_TEST_EXIT_AFTER_MS=120000 "$WORLDWALL_BIN" >"$WORLDWALL_LOG" 2>&1 &
worldwall_pid=$!

# Wait for the client to reach a presented frame and the power adapter to be
# subscribed before asking the operator's session to suspend.
for _ in $(seq 1 120); do
    if grep -q 'trace gpu-frame presented' "$WORLDWALL_LOG" &&
       grep -q 'trace power-service state=available' "$WORLDWALL_LOG"; then
        break
    fi
    if ! kill -0 "$worldwall_pid" 2>/dev/null; then
        wait "$worldwall_pid" || true
        printf '%s\n' 'worldwall exited before the topology test was armed.' >&2
        exit 1
    fi
    sleep 0.25
done

if ! grep -q 'trace gpu-frame presented' "$WORLDWALL_LOG"; then
    printf '%s\n' 'worldwall did not present before the timeout.' >&2
    exit 1
fi

printf '%s\n' 'Armed. Suspending the session for the authorized topology subcase.'
systemctl suspend --no-wall

# Wait for one resume trace and both post-transition state records. The
# helper log is checked below so an ordinary suspend cannot be mistaken for
# this subcase; state queries can finish slightly after the client resumes.
for _ in $(seq 1 120); do
    if grep -q 'trace resume-presented generation=' "$WORLDWALL_LOG" &&
       grep -q 'helper event=resumed' "$WORLDWALL_HELPER_LOG" &&
       grep -q 'helper state output=.*disabled=True' "$WORLDWALL_HELPER_LOG" &&
       grep -q 'helper state output=.*disabled=False' "$WORLDWALL_HELPER_LOG"; then
        break
    fi
    sleep 0.25
done

if ! grep -q 'helper event=entering-sleep' "$WORLDWALL_HELPER_LOG" ||
   ! grep -q 'helper event=resumed' "$WORLDWALL_HELPER_LOG" ||
   ! grep -q 'helper state output=.*disabled=True' "$WORLDWALL_HELPER_LOG" ||
   ! grep -q 'helper state output=.*disabled=False' "$WORLDWALL_HELPER_LOG"; then
    printf '%s\n' 'Topology helper did not prove disable and restore state transitions.' >&2
    exit 1
fi
if ! grep -q 'trace suspend state=paused' "$WORLDWALL_LOG" ||
   ! grep -q 'trace resume-presented generation=' "$WORLDWALL_LOG"; then
    printf '%s\n' 'Worldwall did not record the paused and resumed states.' >&2
    exit 1
fi

printf '%s\n' 'S08 topology subcase passed: helper changed a non-selected output during sleep and Worldwall resumed.'
printf 'worldwall-log=%s\nhelper-log=%s\n' "$WORLDWALL_LOG" "$WORLDWALL_HELPER_LOG"
exit 0
