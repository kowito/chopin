#!/usr/bin/env bash
# CPU profiling script for the Chopin server (macOS + Linux).
#
# macOS: uses `sample` (Instruments CLI)
# Linux: uses `perf record` + `perf report`
#
# Usage:
#   ./benchmarks/profile.sh [port]
set -euo pipefail

PORT="${1:-8080}"
BINARY="./target/release/examples/bench_chopin"
PROFILE_DURATION="${PROFILE_DURATION:-15}"
LOAD_DURATION="${LOAD_DURATION:-20}"
RESULTS_DIR="$(cd "$(dirname "$0")" && pwd)/results"
mkdir -p "$RESULTS_DIR"

if [ ! -f "$BINARY" ]; then
    echo "Building release binary..."
    cargo build --release --example bench_chopin -p chopin-core
fi

# Clean up any leftover server
pkill -f "bench_chopin" 2>/dev/null || true
sleep 1

echo "Starting server on port ${PORT}..."
"$BINARY" &
SERVER_PID=$!
sleep 2

# Verify server is up
if ! curl -sf "http://localhost:${PORT}/plaintext" >/dev/null 2>&1; then
    echo "Server failed to start" >&2
    kill "$SERVER_PID" 2>/dev/null || true
    exit 1
fi
echo "Server started (PID ${SERVER_PID})"

generate_load() {
    for _ in $(seq 1 "$LOAD_DURATION"); do
        for _ in $(seq 1 20); do
            curl -sf "http://localhost:${PORT}/plaintext" >/dev/null &
            curl -sf "http://localhost:${PORT}/json" >/dev/null &
        done
        sleep 1
    done
    wait
}

TIMESTAMP=$(date +%Y%m%d_%H%M%S)

if [[ "$(uname)" == "Darwin" ]]; then
    PROFILE_FILE="${RESULTS_DIR}/profile_${TIMESTAMP}.txt"
    echo "Profiling with macOS sample for ${PROFILE_DURATION}s..."
    generate_load &
    LOAD_PID=$!
    sleep 1
    sample "$SERVER_PID" "${PROFILE_DURATION}" -file "$PROFILE_FILE" -mayDie 2>/dev/null
    wait "$LOAD_PID" 2>/dev/null || true

    echo ""
    echo "Profile saved: ${PROFILE_FILE}"
    echo ""
    echo "Top hot functions:"
    grep -E "^[[:space:]]+[0-9]" "$PROFILE_FILE" | sort -rn | head -20 || head -50 "$PROFILE_FILE"

elif [[ "$(uname)" == "Linux" ]]; then
    PROFILE_FILE="${RESULTS_DIR}/perf_${TIMESTAMP}.data"
    echo "Profiling with perf for ${PROFILE_DURATION}s..."
    generate_load &
    LOAD_PID=$!
    sleep 1
    perf record -g -p "$SERVER_PID" -o "$PROFILE_FILE" -- sleep "$PROFILE_DURATION" 2>/dev/null || true
    wait "$LOAD_PID" 2>/dev/null || true

    echo ""
    echo "Profile data: ${PROFILE_FILE}"
    echo ""
    echo "Top hot functions:"
    perf report -i "$PROFILE_FILE" --stdio --no-children | head -40 || true
else
    echo "Profiling not supported on $(uname)"
fi

kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true
echo ""
echo "Done. Profile saved in ${RESULTS_DIR}/"
