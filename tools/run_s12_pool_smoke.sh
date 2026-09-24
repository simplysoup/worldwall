#!/usr/bin/env bash
# S12-AC4/AC5 live pool-mode smoke run against a nested, real Hyprland instance.
#
# The nested compositor uses the outer Wayland session as its display backend;
# it is not a mock and it does not modify the outer compositor configuration,
# the production wallpaper process, or any session configuration. The smoke
# child runs with a private XDG_RUNTIME_DIR so its control socket is distinct
# from any running Worldwall instance, and every control command uses
# `worldwallctl --socket` against that test socket.
#
# Package assets come from the private, untracked derived fixtures under
# fixtures/untracked/effectgames (see docs/evidence); smoke packages are
# assembled as clearly-labelled local test material in a temporary directory
# and are never written back into the repository. No weather provider request
# is possible: every smoke config leaves `[weather]` disabled.
#
# Usage: python3-availability + debug build; run from a Wayland session.
set -Eeuo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
: "${WAYLAND_DISPLAY:?run this smoke from a Wayland session}"
outer_wayland_display=$WAYLAND_DISPLAY
outer_runtime_dir=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
nested_wayland_display="worldwall-s12-nested-$$"

fixtures="$repo_root/fixtures/untracked/effectgames"
[[ -d "$fixtures/v08-jungle-waterfall--cloudy" && -d "$fixtures/canvascycle-v25heat/worldwall-scene-package" ]] || {
    printf 'private derived fixtures are unavailable under %s; smoke not run\n' "$fixtures" >&2
    exit 2
}

work=$(mktemp -d)
client_pid=''
child_pid=''
signature=''
socket=''
keep_logs=0

cleanup() {
    local status=$?
    set +e
    [[ -n "$client_pid" ]] && { kill "$client_pid" 2>/dev/null; wait "$client_pid" 2>/dev/null; }
    [[ -n "$child_pid" ]] && { kill "$child_pid" 2>/dev/null; wait "$child_pid" 2>/dev/null; }
    if (( status != 0 )); then
        keep_logs=1
        printf 'S12 pool smoke failed; logs retained at %s\n' "$work" >&2
    fi
    # Optional durable artifacts (screenshots/logs) for evidence records.
    if [[ -n "${WORLDWALL_SMOKE_ARTIFACTS:-}" ]]; then
        mkdir -p "$WORLDWALL_SMOKE_ARTIFACTS"
        cp -f "$work"/*.png "$work"/worldwall-*.log "$work"/scene-pools.toml \
            "$WORLDWALL_SMOKE_ARTIFACTS/" 2>/dev/null
        printf 'S12 pool smoke artifacts copied to %s\n' "$WORLDWALL_SMOKE_ARTIFACTS" >&2
    fi
    (( keep_logs )) || rm -rf "$work"
    exit "$status"
}
trap cleanup EXIT INT TERM

fail() {
    printf 'SMOKE ASSERTION FAILED: %s\n' "$1" >&2
    exit 1
}

# --------------------------------------------------------------------------
# Assemble smoke registry, library, and sidecar from private derived fixtures.

mkdir -p "$work/run" "$work/registry" "$work/library"
chmod 700 "$work/run"

write_package() {
    local dir=$1 id=$2 source=$3
    mkdir -p "$dir"
    cp "$fixtures/$source/scene.png" "$fixtures/$source/palette.png" "$dir/"
    [[ -f "$fixtures/$source/animation.toml" ]] && cp "$fixtures/$source/animation.toml" "$dir/"
    [[ -f "$fixtures/$source/time_of_day.toml" ]] && cp "$fixtures/$source/time_of_day.toml" "$dir/"
    [[ -f "$fixtures/$source/time_of_day.png" ]] && cp "$fixtures/$source/time_of_day.png" "$dir/"
    cat >"$dir/scene.toml" <<EOF
schema_version = 1
id = "$id"
image = "scene.png"
palette = "palette.png"
animation = "animation.toml"
time_of_day = "time_of_day.toml"
EOF
}

# Wildcard-tagged smoke packages: pool rules are the only availability filter.
write_package "$work/registry/smoke-a" smoke-a v08-jungle-waterfall--cloudy
write_package "$work/registry/smoke-b" smoke-b v19-mountain-stream--cloudy
write_package "$work/library/smoke-c" smoke-c vw3b-monolith-plains--clear
cat >"$work/registry/registry.toml" <<'EOF'
schema_version = 1
scenes = ["smoke-a", "smoke-b"]
fallback = "smoke-a"
EOF

# The real private capture package round-trips unchanged into the library.
cp -r "$fixtures/canvascycle-v25heat/worldwall-scene-package" "$work/library/canvascycle-v25heat"

python3 - "$work/library" <<'PY'
import hashlib, pathlib, sys

root = pathlib.Path(sys.argv[1])

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

rows = []

def entry(id_, package, collection, world, appearance, assets, provenance):
    fields = [
        f'id = "{id_}"',
        f'package = "{package}"',
        f'source_collection = "{collection}"',
        f'world = "{world}"',
        f'appearance = "{appearance}"',
    ]
    for key, name in assets:
        fields.append(f'{key} = "{name}"')
        fields.append(f'{key}_sha256 = "{sha(root / package / name)}"')
    fields.extend(provenance)
    rows.append("\n[[scene]]\n" + "\n".join(fields) + "\n")

entry(
    "smoke-c", "smoke-c", "effectgames", "vw3b-monolith-plains", "clear",
    [("image", "scene.png"), ("palette", "palette.png"),
     ("animation", "animation.toml"), ("time_of_day", "time_of_day.toml"),
     ("time_of_day_atlas", "time_of_day.png")],
    ['source_format = "effectgames-world-scene-derivation-v1"',
     f'source_sha256 = "{sha(root / "smoke-c" / "animation.toml")}"',
     'source_url = "https://www.effectgames.com/demos/worlds/cache/VW3B-MONOLITH.js"',
     'source_page = "https://www.effectgames.com/demos/worlds/scenes.js"',
     'source_title = "VW3B Monolith Plains (clear) - private derived smoke package"',
     'license_state = "unreviewed"'],
)
entry(
    "canvascycle-v25heat", "canvascycle-v25heat", "canvascycle", "unknown", "unknown",
    [("image", "scene.png"), ("palette", "palette.png"),
     ("source_cycles", "source-cycles.toml")],
    ['cycle_semantics = "unmapped-source-native"',
     'source_format = "canvascycle-legacy-object-literal-v1"',
     'source_filename = "V25HEAT.LBM"',
     'source_sha256 = "2d0ee7fa2d83ab986514e76db05a88f6735a6186dde28fb6466a6fd45ed2305f"',
     'source_url = "https://www.effectgames.com/demos/canvascycle/images/V25HEAT.LBM.json"',
     'source_page = "https://www.effectgames.com/demos/canvascycle/"',
     'source_title = "Desert Heat Wave - Day (V25HEAT)"',
     'license_state = "unreviewed"'],
)
(root / "library.toml").write_text("schema_version = 1\n" + "".join(rows))
print("library catalog generated")
PY

cat >"$work/library/scene-pools.toml" <<'EOF'
schema_version = 1

[[pool]]
id = "smoke-library"
source = "library"
enabled = true
scene_ids = ["smoke-c", "canvascycle-v25heat"]

[[pool]]
id = "smoke-registry"
source = "registry"
enabled = true
scene_ids = ["smoke-a", "smoke-b"]

[[scene_rule]]
scene_id = "smoke-a"
enabled = true

[[scene_rule]]
scene_id = "smoke-b"
enabled = true

[[scene_rule]]
scene_id = "smoke-c"
enabled = true
weather_any = ["clear"]

[[scene_rule]]
scene_id = "canvascycle-v25heat"
enabled = false
EOF

cat >"$work/worldwall.toml" <<EOF
schema_version = 5
fps = 15
registry_dir = "$work/registry"
scene_pools_file = "$work/library/scene-pools.toml"
animation_speed_multiplier = 0.5
EOF

ctl() {
    WORLDWALL_CONFIG="$work/worldwall.toml" \
        "$repo_root/target/debug/worldwallctl" --socket "$work/run/worldwall.sock" "$@"
}

# --------------------------------------------------------------------------
# Nested Hyprland on the outer session (same pattern as tools/run_nested_s01.sh).

cat >"$work/hyprland.conf" <<'EOF'
monitor = ,1280x720@60,0x0,1
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
        kill -0 "$child_pid" 2>/dev/null || { cat "$log" >&2; return 1; }
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

start_client() {
    local log=$1
    # Absolute socket path: the child's private XDG_RUNTIME_DIR must not
    # shadow the nested compositor's socket name.
    WAYLAND_DISPLAY="$outer_runtime_dir/$socket" \
        XDG_RUNTIME_DIR="$work/run" \
        WORLDWALL_CONFIG="$work/worldwall.toml" \
        "$repo_root/target/debug/worldwall" >"$log" 2>&1 &
    client_pid=$!
    for _ in $(seq 1 150); do
        grep -q 'trace gpu-frame presented' "$log" 2>/dev/null && return 0
        kill -0 "$client_pid" 2>/dev/null || return 2
        sleep 0.1
    done
    return 1
}

stop_client() {
    # `wait` reports the killed client's status (143); that is expected here.
    [[ -n "$client_pid" ]] && { kill "$client_pid" 2>/dev/null; wait "$client_pid" 2>/dev/null || true; client_pid=''; }
}

scene_status() {
    ctl status | python3 -c 'import json, sys; d = json.load(sys.stdin); print(d["data"]["scene"], d["data"]["selection_generation"])'
}

# --------------------------------------------------------------------------
printf '== Phase A: pool mode with unknown environment inputs ==\n'
start_nested "$work/hyprland-a.log"
start_client "$work/worldwall-a.log" || { cat "$work/worldwall-a.log" >&2; fail "smoke client did not present"; }

mapfile -t _startup_trace < <(scene_status)
read -r active generation <<<"${_startup_trace[0]}"
printf 'startup selected %s (generation %s)\n' "$active" "$generation"
case "$active" in
    smoke-a | smoke-b) ;;
    *) fail "startup selected $active outside the eligible set {smoke-a, smoke-b}" ;;
esac
grep -q "trace scene-source=registry id=$active trigger=startup" "$work/worldwall-a.log" \
    || fail "startup trace missing for $active"

weather_state=$(ctl weather | python3 -c 'import json, sys; print(json.load(sys.stdin)["data"]["state"])')
[[ "$weather_state" == "disabled" ]] || fail "weather provider is not disabled ($weather_state)"

# `next` cycles the eligible set deterministically with wrap.
first=$(ctl next | python3 -c 'import json, sys; print(json.load(sys.stdin)["data"]["scene"])')
[[ "$first" != "$active" ]] || fail "next did not leave the active scene"[[ "$first" == "smoke-a" || "$first" == "smoke-b" ]] || fail "next left the eligible set: $first"
second=$(ctl next | python3 -c 'import json, sys; print(json.load(sys.stdin)["data"]["scene"])')
[[ "$second" == "$active" ]] || fail "next did not wrap ($second != $active)"

# Manual IDs are the explicit override of pool and rule state.
ctl scene smoke-c >/dev/null
[[ "$(scene_status | cut -d' ' -f1)" == "smoke-c" ]] || fail "manual override to smoke-c failed"
ctl scene canvascycle-v25heat >/dev/null
[[ "$(scene_status | cut -d' ' -f1)" == "canvascycle-v25heat" ]] || fail "manual override to canvascycle-v25heat failed"

# Automatic paths return to the eligible set only.
back=$(ctl next | python3 -c 'import json, sys; print(json.load(sys.stdin)["data"]["scene"])')
[[ "$back" == "smoke-a" || "$back" == "smoke-b" ]] || fail "next after override selected ineligible $back"
printf 'manual override and filtered next verified (returned to %s)\n' "$back"

# Capture the nested compositor window itself (pool-selected scene rendering)
# plus the surrounding outer session (production wallpaper undisturbed).
nested_box=$(hyprctl clients -j 2>/dev/null \
    | CHILD_PID="$child_pid" python3 -c '
import json, os, sys
for item in json.load(sys.stdin):
    if item.get("pid") == int(os.environ["CHILD_PID"]):
        at, size = item["at"], item["size"]
        print(f"{at[0]},{at[1]} {size[0]}x{size[1]}")
        break
' || true)
if [[ -n "$nested_box" ]]; then
    grim -g "$nested_box" "$work/nested-pool-mode.png" 2>/dev/null || true
fi
grim "$work/outer-session-during-smoke.png" 2>/dev/null || true
sleep 0.2

# `reload-config` applies rule changes atomically and never selects.
before=$(scene_status)
cat >"$work/library/scene-pools.toml" <<'EOF'
schema_version = 1

[[pool]]
id = "smoke-library"
source = "library"
enabled = true
scene_ids = ["smoke-c", "canvascycle-v25heat"]

[[pool]]
id = "smoke-registry"
source = "registry"
enabled = true
scene_ids = ["smoke-a", "smoke-b"]

[[scene_rule]]
scene_id = "smoke-a"
enabled = false

[[scene_rule]]
scene_id = "smoke-b"
enabled = false

[[scene_rule]]
scene_id = "smoke-c"
enabled = true
temperature_min_c = 5.0

[[scene_rule]]
scene_id = "canvascycle-v25heat"
enabled = false
EOF
reload=$(ctl reload-config | python3 -c 'import json, sys; print(json.load(sys.stdin)["data"])')
grep -q "'reselected': False" <(printf '%s' "$reload") || fail "reload-config reported a reselection: $reload"
after=$(scene_status)
[[ "$before" == "$after" ]] || fail "reload-config changed scene/generation ($before -> $after)"
printf 'reload-config applied without selecting (scene/generation %s unchanged)\n' "$after"

# With every automatic candidate filtered out, `next` is a no-op with a reason.
noop=$(ctl next)
printf '%s' "$noop" | grep -q '"outcome": "noop"' || fail "next did not report a no-op: $noop"
printf '%s' "$noop" | grep -q 'no eligible scene for next' || fail "no-op reason missing: $noop"
[[ "$(scene_status | cut -d' ' -f1)" == "${after%% *}" ]] || fail "no-op next changed the resident scene"

# A malformed sidecar keeps the previous live rules (still no eligible scene).
printf 'schema_version = 99\n' >"$work/library/scene-pools.toml"
invalid=$(ctl reload-config 2>&1 || true)
printf '%s' "$invalid" | grep -q 'invalid_config' || fail "invalid sidecar was not rejected: $invalid"
broken=$(ctl next)
printf '%s' "$broken" | grep -q 'no eligible scene for next' || fail "failed reload changed live rules: $broken"

# No clock/weather/config refresh ever selected: only explicit commands.
spontaneous=$(grep -c 'trigger=ClockTick\|trigger=WeatherUpdated\|trigger=ConfigChanged\|trigger=OutputChanged' "$work/worldwall-a.log" || true)
[[ "$spontaneous" == "0" ]] || fail "non-trigger events selected ($spontaneous traces)"
[[ "$(grep -c 'trigger=startup' "$work/worldwall-a.log")" == "1" ]] || fail "startup selection count unexpected"
stop_client

# --------------------------------------------------------------------------
printf '== Phase B: startup with no eligible scene fails closed ==\n'
cat >"$work/library/scene-pools.toml" <<'EOF'
schema_version = 1

[[pool]]
id = "smoke-library"
source = "library"
enabled = false
scene_ids = ["smoke-c", "canvascycle-v25heat"]

[[pool]]
id = "smoke-registry"
source = "registry"
enabled = false
scene_ids = ["smoke-a", "smoke-b"]
EOF
set +e
WAYLAND_DISPLAY="$outer_runtime_dir/$socket" \
    XDG_RUNTIME_DIR="$work/run" \
    WORLDWALL_CONFIG="$work/worldwall.toml" \
    "$repo_root/target/debug/worldwall" >"$work/worldwall-b.log" 2>&1
phase_b_status=$?
set -e
[[ "$phase_b_status" != "0" ]] || fail "no-eligible startup exited successfully"
grep -q 'startup selection unavailable: no eligible scene' "$work/worldwall-b.log" \
    || fail "bounded startup diagnostic missing: $(cat "$work/worldwall-b.log")"
if grep -q 'trace gpu-frame presented' "$work/worldwall-b.log"; then
    fail "no-eligible startup presented a frame before failing closed"
fi
printf 'no-eligible startup exited %s before presentation with the bounded diagnostic\n' "$phase_b_status"

# --------------------------------------------------------------------------
printf '== Phase C: known-input match and mismatch with explicit test location ==\n'
cat >"$work/worldwall.toml" <<EOF
schema_version = 5
fps = 15
registry_dir = "$work/registry"
scene_pools_file = "$work/library/scene-pools.toml"
animation_speed_multiplier = 0.5

# Smoke-test values only (mockup data, not an inferred or recommended location).
[location]
latitude = 0.0
longitude = 0.0
timezone = "Etc/UTC"
EOF
cat >"$work/library/scene-pools.toml" <<'EOF'
schema_version = 1

[[pool]]
id = "smoke-library"
source = "library"
enabled = true
scene_ids = ["smoke-c", "canvascycle-v25heat"]

[[pool]]
id = "smoke-registry"
source = "registry"
enabled = true
scene_ids = ["smoke-a", "smoke-b"]

[[scene_rule]]
scene_id = "smoke-a"
enabled = true
time_of_day = ["dawn", "day", "dusk", "night", "polar_day", "polar_night"]

[[scene_rule]]
scene_id = "smoke-b"
enabled = true
time_of_day = ["polar_day"]

[[scene_rule]]
scene_id = "smoke-c"
enabled = true
weather_any = ["clear"]

[[scene_rule]]
scene_id = "canvascycle-v25heat"
enabled = true
EOF
start_client "$work/worldwall-c.log" || { cat "$work/worldwall-c.log" >&2; fail "phase C client did not present"; }
grep -q 'civil-time configured=true' "$work/worldwall-c.log" \
    || fail "explicit test location did not configure civil time"
c_active=$(scene_status | cut -d' ' -f1)
case "$c_active" in
    smoke-a | canvascycle-v25heat) ;;
    *) fail "known-time startup selected $c_active; expected an always-matching candidate" ;;
esac
c_next=$(ctl next | python3 -c 'import json, sys; print(json.load(sys.stdin)["data"]["scene"])')
case "$c_next" in
    smoke-a | canvascycle-v25heat) ;;
    *) fail "known-time next selected ineligible $c_next" ;;
esac
printf 'known-input criteria matched (%s -> %s); polar_day/unknown-weather candidates stayed filtered\n' \
    "$c_active" "$c_next"
stop_client
kill "$child_pid" 2>/dev/null || true
wait "$child_pid" 2>/dev/null || true
child_pid=''

printf 'S12 pool-mode smoke passed (renderer=%s)\n' "${WLR_RENDERER:-gles2}"
