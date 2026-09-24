#!/usr/bin/env bash
# Run the Wayland lifecycle and one GPU frame against a nested, real Hyprland instance.
# The nested compositor uses the outer Wayland session as its display backend;
# it is not a mock and it does not modify the outer compositor configuration.
#
# This harness exercises the preserved single-output S01/S02 baseline through
# the explicit WORLDWALL_SINGLE_OUTPUT_BASELINE seam (see FM-0016): its
# teardown assertions target that path's contract, which the production
# run_multi() entry point replaced with the S09 multi-output behavior.
set -Eeuo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
: "${WAYLAND_DISPLAY:?run this harness from a Wayland session}"
outer_wayland_display=$WAYLAND_DISPLAY
nested_wayland_display="worldwall-s02-nested-$$"

work=$(mktemp -d)
child_pid=''
client_pid=''
signature=''
socket=''
keep_logs=0

cleanup() {
    local status=$?
    set +e
    if [[ -n "$client_pid" ]]; then
        kill "$client_pid" 2>/dev/null || true
        wait "$client_pid" 2>/dev/null || true
    fi
    if [[ -n "$child_pid" ]]; then
        kill "$child_pid" 2>/dev/null || true
        wait "$child_pid" 2>/dev/null || true
    fi
    if (( status != 0 )); then
        keep_logs=1
        printf 'nested S01 harness failed; logs retained at %s\n' "$work" >&2
    fi
    if (( ! keep_logs )); then
        rm -rf "$work"
    fi
    exit "$status"
}
trap cleanup EXIT INT TERM

cat >"$work/hyprland.conf" <<'EOF'
monitor = ,1920x1080@60,0x0,1
misc {
    disable_hyprland_logo = true
    disable_splash_rendering = true
}
EOF

cargo build --locked --manifest-path "$repo_root/Cargo.toml" >/dev/null
"${CC:-cc}" -shared -fPIC -o "$work/nested-wayland-connect.so" \
    "$repo_root/tools/nested_wayland_connect.c" -ldl

start_nested() {
    local log=$1
    (
        cd "$repo_root"
        WAYLAND_DISPLAY="$nested_wayland_display" \
            WORLDWALL_UPSTREAM_DISPLAY="$outer_wayland_display" \
            LD_PRELOAD="$work/nested-wayland-connect.so" \
            WLR_BACKENDS=wayland WLR_RENDERER="${WLR_RENDERER:-gles2}" \
            Hyprland --config "$work/hyprland.conf"
    ) >"$log" 2>&1 &
    child_pid=$!
    signature=''
    socket=''

    for _ in $(seq 1 100); do
        local line
        line=$(hyprctl instances -j 2>/dev/null \
            | CHILD_PID="$child_pid" python3 -c '
import json, os, sys
data = json.load(sys.stdin)
pairs = [
    (item.get("instance", ""), item.get("wl_socket", ""))
    for item in data
    if int(item.get("pid", -1)) == int(os.environ["CHILD_PID"])
]
print(*[f"{signature} {socket}" for signature, socket in pairs], sep="\n")
' | tail -1 || true)
        if [[ -n "$line" ]]; then
            read -r signature socket <<<"$line"
            break
        fi
        kill -0 "$child_pid" 2>/dev/null || {
            cat "$log" >&2
            return 1
        }
        sleep 0.1
    done

    [[ -n "$signature" && -n "$socket" ]]
    [[ "$socket" != "$outer_wayland_display" ]] || {
        printf 'nested compositor reused outer Wayland socket %s\n' "$socket" >&2
        return 1
    }
    printf 'nested instance pid=%s socket=%s outer-socket=%s\n' \
        "$child_pid" "$socket" "$outer_wayland_display"
}

stop_nested() {
    if [[ -n "$child_pid" ]]; then
        kill "$child_pid" 2>/dev/null || true
        wait "$child_pid" 2>/dev/null || true
        child_pid=''
    fi
}

start_client() {
    local log=$1
    WAYLAND_DISPLAY="$socket" \
        WORLDWALL_SINGLE_OUTPUT_BASELINE=1 \
        "$repo_root/target/debug/worldwall" >"$log" 2>&1 &
    client_pid=$!
    for _ in $(seq 1 100); do
        grep -q 'trace configure ' "$log" 2>/dev/null && return 0
        kill -0 "$client_pid" 2>/dev/null || {
            cat "$log" >&2
            return 1
        }
        sleep 0.1
    done
    cat "$log" >&2
    return 1
}

wait_client_presented() {
    local log=$1
    for _ in $(seq 1 200); do
        grep -q 'trace gpu-frame presented' "$log" 2>/dev/null && return 0
        kill -0 "$client_pid" 2>/dev/null || {
            cat "$log" >&2
            return 1
        }
        sleep 0.1
    done
    cat "$log" >&2
    return 1
}

wait_client_exit() {
    for _ in $(seq 1 100); do
        kill -0 "$client_pid" 2>/dev/null || break
        sleep 0.1
    done
    if kill -0 "$client_pid" 2>/dev/null; then
        cat "$1" >&2
        return 1
    fi
    wait "$client_pid" 2>/dev/null || true
    client_pid=''
}

# First exercise compositor-driven layer close and selected-output removal.
start_nested "$work/hyprland-output-removal.log"
output=$(hyprctl --instance "$signature" monitors -j \
    | python3 -c 'import json, sys; print(next(item["name"] for item in json.load(sys.stdin) if not item.get("disabled", False)))')
start_client "$work/worldwall-output-removal.log"
wait_client_presented "$work/worldwall-output-removal.log"
hyprctl --instance "$signature" output remove "$output" >/dev/null
wait_client_exit "$work/worldwall-output-removal.log"
grep -q 'trace layer-closed state=closing owners-dropped=true' "$work/worldwall-output-removal.log"
grep -q 'trace selected-output-removed' "$work/worldwall-output-removal.log"
grep -q 'trace gpu-frame presented' "$work/worldwall-output-removal.log"
grep -q 'trace owners-dropped gpu-before-layer-before-selected-output=true' "$work/worldwall-output-removal.log"
cat "$work/worldwall-output-removal.log"
stop_nested

# Start a fresh nested compositor and exercise a real Wayland disconnect.
start_nested "$work/hyprland-disconnect.log"
start_client "$work/worldwall-disconnect.log"
wait_client_presented "$work/worldwall-disconnect.log"
kill "$child_pid"
wait "$child_pid" 2>/dev/null || true
child_pid=''
wait_client_exit "$work/worldwall-disconnect.log"
grep -q 'trace compositor-disconnect' "$work/worldwall-disconnect.log"
grep -q 'trace owners-dropped gpu-before-layer-before-selected-output=true' "$work/worldwall-disconnect.log"
cat "$work/worldwall-disconnect.log"

printf 'nested Hyprland S01 lifecycle + S02 presentation checks passed (renderer=%s)\n' "${WLR_RENDERER:-gles2}"
