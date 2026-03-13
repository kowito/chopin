#!/usr/bin/env bash
# Compare epoll vs io_uring throughput side-by-side.
#
# This script launches each server variant in the background, waits for it to
# become ready, benchmarks it, then kills it before starting the next.
#
# Usage:
#   ./benchmarks/compare.sh
#
# Environment overrides: same as benchmarks/run.sh
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RESULTS_DIR="${SCRIPT_DIR}/results"
mkdir -p "$RESULTS_DIR"

HOST="${BENCH_HOST:-127.0.0.1}"
PORT="${BENCH_PORT:-8080}"
DURATION="${BENCH_DURATION:-30s}"
WARMUP="${BENCH_WARMUP:-5s}"
CONNECTIONS="${BENCH_CONNECTIONS:-512}"
if command -v nproc &>/dev/null; then
    CPU_COUNT=$(nproc)
elif command -v sysctl &>/dev/null; then
    CPU_COUNT=$(sysctl -n hw.logicalcpu 2>/dev/null || echo 8)
else
    CPU_COUNT=8
fi
THREADS="${BENCH_THREADS:-$CPU_COUNT}"
URL="http://${HOST}:${PORT}/plaintext"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

if ! command -v wrk &>/dev/null; then
    echo "Error: wrk not found. Install it first (brew install wrk)."
    exit 1
fi

wait_for_server() {
    local max=20 n=0
    while ! curl -sf --connect-timeout 1 "${URL}" >/dev/null 2>&1; do
        (( n++ ))
        if (( n >= max )); then
            echo "Server did not start within ${max}s" >&2
            return 1
        fi
        sleep 1
    done
}

run_variant() {
    local variant="$1"
    local extra_flags="${2:-}"
    local outfile="${RESULTS_DIR}/bench_${variant}_${TIMESTAMP}.txt"

    echo ""
    echo "================================================================"
    echo "  Variant: ${variant}"
    echo "================================================================"

    # Build & start server
    echo "Building ${variant} server..."
    # shellcheck disable=SC2086
    cargo build --release --example bench_chopin -p chopin-core ${extra_flags} 2>&1 | \
        grep -E "^(Compiling|Finished|error)" || true

    echo "Starting ${variant} server on ${HOST}:${PORT}..."
    if [ "${variant}" = "iouring" ]; then
        CHOPIN_IO_URING=1 ./target/release/examples/bench_chopin &
    else
        ./target/release/examples/bench_chopin &
    fi
    local SERVER_PID=$!

    # Wait for server to come up
    if ! wait_for_server; then
        kill "$SERVER_PID" 2>/dev/null || true
        return 1
    fi

    # Warmup
    echo "Warming up..."
    wrk -t"${THREADS}" -c"${CONNECTIONS}" -d"${WARMUP}" "${URL}" >/dev/null 2>&1 || true
    sleep 1

    # Benchmark
    echo "Benchmarking..."
    wrk -t"${THREADS}" -c"${CONNECTIONS}" -d"${DURATION}" --latency "${URL}" | tee "${outfile}"

    # Stop server
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    sleep 1

    echo "Results: ${outfile}"
}

echo "Chopin — epoll vs io_uring comparison"
echo "Threads: ${THREADS}  Connections: ${CONNECTIONS}  Duration: ${DURATION}"

run_variant "epoll"

# io_uring is Linux-only
if [[ "$(uname)" == "Linux" ]]; then
    run_variant "iouring" "--features io-uring"
else
    echo ""
    echo "Note: io_uring benchmarks are Linux-only. Skipping on $(uname)."
fi

echo ""
echo "================================================================"
echo "  Summary"
echo "================================================================"
for f in "${RESULTS_DIR}"/bench_*_"${TIMESTAMP}".txt; do
    variant=$(basename "$f" | sed "s/bench_//;s/_${TIMESTAMP}.txt//")
    rps=$(grep -oE '[0-9]+\.[0-9]+k? Requests/sec' "$f" | head -1 || echo "n/a")
    echo "  ${variant}: ${rps}"
done
echo ""
echo "Raw results in: ${RESULTS_DIR}/"
