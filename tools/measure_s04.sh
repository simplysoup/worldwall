#!/bin/sh
# Collect the prescribed S04 single-output baseline. This is observational data:
# numeric release limits are assigned to S11, not invented here.
set -eu

if [ "$#" -ne 3 ]; then
    echo "usage: $0 SCENE_DIR FPS OUTPUT_DIR" >&2
    exit 2
fi

scene_dir=$1
fps=$2
output_dir=$3
binary=${WORLDWALL_BIN:-target/debug/worldwall}
interval_seconds=${WORLDWALL_SAMPLE_INTERVAL_SECONDS:-1}
warmup_seconds=${WORLDWALL_WARMUP_SECONDS:-60}
sample_seconds=${WORLDWALL_SAMPLE_SECONDS:-300}
sample_count=${WORLDWALL_SAMPLE_COUNT:-3}

case "$fps" in
    15|30) ;;
    *) echo "FPS must be 15 or 30" >&2; exit 2 ;;
esac
[ -x "$binary" ] || { echo "missing executable $binary; run cargo build --locked first" >&2; exit 2; }
[ -d "$scene_dir" ] || { echo "missing scene directory $scene_dir" >&2; exit 2; }

mkdir -p "$output_dir"
trace="$output_dir/worldwall-trace.log"
metrics="$output_dir/resource-samples.csv"
: > "$trace"
printf 'sample,elapsed_seconds,cpu_percent,rss_kib,gpu_util_percent,power_watts,vram_mib,animation_frames,last_phase_seconds,scene_upload_count\n' > "$metrics"

# The debug-only deadline gives the process a clean owner-drop path after the
# final sample. A shell timeout remains the outer safety bound.
duration_seconds=$((warmup_seconds + sample_seconds * sample_count + 2))
duration_ms=$((duration_seconds * 1000))
WORLDWALL_SCENE_DIR="$scene_dir" WORLDWALL_FPS="$fps" \
    WORLDWALL_TEST_EXIT_AFTER_MS="$duration_ms" "$binary" >"$trace" 2>&1 &
pid=$!
cleanup() {
    if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

sleep "$warmup_seconds"
for sample in $(seq 1 "$sample_count"); do
    end=$(( $(date +%s) + sample_seconds ))
    while [ "$(date +%s)" -lt "$end" ]; do
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "worldwall exited before sample $sample" >&2
            exit 1
        fi
        now=$(date +%s.%N)
        ps_values=$(ps -p "$pid" -o %cpu=,rss= | awk '{print $1 "," $2}')
        cpu=$(printf '%s' "$ps_values" | cut -d, -f1)
        rss=$(printf '%s' "$ps_values" | cut -d, -f2)
        gpu=$(nvidia-smi --query-gpu=utilization.gpu,power.draw,memory.used \
            --format=csv,noheader,nounits 2>/dev/null | tr -d ' ' | tr '\n' ';' | sed 's/;$//')
        frames=$(grep -c 'trace animation-frame' "$trace" 2>/dev/null || true)
        last_frame=$(grep 'trace animation-frame' "$trace" 2>/dev/null | tail -1 || true)
        phase=$(printf '%s' "$last_frame" | sed -n 's/.*phase-seconds=\([^ ]*\).*/\1/p')
        uploads=$(printf '%s' "$last_frame" | sed -n 's/.*scene-upload-count=\([^ ]*\).*/\1/p')
        printf '%s,%s,%s,%s,%s,%s,%s,%s\n' \
            "$sample" "$now" "${cpu:-unavailable}" "${rss:-unavailable}" \
            "${gpu:-unavailable}" "${frames:-0}" "${phase:-unavailable}" \
            "${uploads:-unavailable}" >> "$metrics"
        sleep "$interval_seconds"
    done
done

wait "$pid"
trap - EXIT INT TERM
printf 'worldwall-exit-status=0\n' >> "$trace"
