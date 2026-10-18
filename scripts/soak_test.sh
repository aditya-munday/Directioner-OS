#!/usr/bin/env bash
# Soak and leak test runner for Directioner-OS daemon (TODO 25)
# Usage: ./scripts/soak_test.sh [--duration SECS] [--interval SECS] [--pid PID]

set -euo pipefail

DURATION=${1:-60}
INTERVAL=${2:-1}
TARGET_PID=${3:-$$}
OUTPUT_DIR="${CARGO_TARGET_DIR:-target}/soak_artifacts"
mkdir -p "$OUTPUT_DIR"

CSV_FILE="$OUTPUT_DIR/soak_${TARGET_PID}_$(date +%s).csv"
echo "elapsed_secs,rss_kb,open_fds,thread_count" > "$CSV_FILE"

echo "[SOAK] Starting soak test for PID $TARGET_PID for ${DURATION}s (sampling every ${INTERVAL}s)..."
START_TIME=$(date +%s)
ELAPSED=0

while [ "$ELAPSED" -lt "$DURATION" ]; do
    if ! kill -0 "$TARGET_PID" 2>/dev/null; then
        echo "[SOAK] Target PID $TARGET_PID exited early!"
        exit 1
    fi

    # Read /proc/$TARGET_PID/statm and /status
    if [ -f "/proc/$TARGET_PID/statm" ]; then
        PAGES=$(awk '{print $2}' "/proc/$TARGET_PID/statm")
        RSS_KB=$(( PAGES * 4 ))
    else
        RSS_KB=0
    fi

    if [ -d "/proc/$TARGET_PID/fd" ]; then
        FDS=$(ls -1 "/proc/$TARGET_PID/fd" 2>/dev/null | wc -l)
    else
        FDS=0
    fi

    if [ -d "/proc/$TARGET_PID/task" ]; then
        THREADS=$(ls -1 "/proc/$TARGET_PID/task" 2>/dev/null | wc -l)
    else
        THREADS=1
    fi

    echo "$ELAPSED,$RSS_KB,$FDS,$THREADS" >> "$CSV_FILE"

    sleep "$INTERVAL"
    NOW=$(date +%s)
    ELAPSED=$(( NOW - START_TIME ))
done

echo "[SOAK] Soak completed. Output saved to $CSV_FILE"
