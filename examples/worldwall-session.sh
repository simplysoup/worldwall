#!/bin/sh
# Install as ~/.local/bin/worldwall-session; call once from the compositor.
# Resume is handled inside Worldwall via logind, not by launching another copy.
set -eu
umask 077

: "${HOME:?HOME is required}"
: "${XDG_RUNTIME_DIR:?run inside the user Wayland session}"
: "${WAYLAND_DISPLAY:?run inside the user Wayland session}"
config=${WORLDWALL_CONFIG:-${XDG_CONFIG_HOME:-$HOME/.config}/worldwall/worldwall.toml}
binary=${XDG_DATA_HOME:-$HOME/.local/share}/worldwall/bin/worldwall
state=${XDG_STATE_HOME:-$HOME/.local/state}/worldwall

[ -r "$config" ] || { echo "worldwall-session: config not readable: $config" >&2; exit 1; }
[ -x "$binary" ] || { echo "worldwall-session: binary not executable: $binary" >&2; exit 1; }
command -v flock >/dev/null || { echo 'worldwall-session: flock is required' >&2; exit 1; }
# FD 9 survives exec and releases automatically on exit/crash/compositor shutdown.
# Do not unlink the lock file while the session is running.
exec 9>"$XDG_RUNTIME_DIR/worldwall-session.lock"
if flock -n -E 75 9; then
    :
else
    result=$?
    [ "$result" -eq 75 ] && exit 0
    exit "$result"
fi

mkdir -p "$state"
export WORLDWALL_CONFIG="$config"
# A legacy fixed-scene or diagnostic override must not bypass the user config.
unset WORLDWALL_SCENE_DIR WORLDWALL_LAYER_ONLY WORLDWALL_SOLID_COLOR \
    WORLDWALL_TEST_EXIT_AFTER_MS WORLDWALL_TEST_UTC_SECONDS \
    WORLDWALL_TEST_WITHHOLD_CALLBACK WORLDWALL_TEST_WITHHOLD_OUTPUT \
    WORLDWALL_TEST_SURFACE_ERROR
# Per-frame stdout is intentionally discarded in ordinary unattended use.
# For a bounded diagnostic run only, set WORLDWALL_SESSION_TRACE to a private file.
exec "$binary" >"${WORLDWALL_SESSION_TRACE:-/dev/null}" 2>>"$state/session-errors.log"
