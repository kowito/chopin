#!/usr/bin/env bash
# A.7: Benchmark io_uring vs epoll on Linux.
# Requires: wrk (https://github.com/wg/wrk), a running chopin server.
#
# Usage:
#   # Build both variants:
#   cargo build --release --example tfb                      # epoll
#   cargo build --release --example tfb --features io-uring  # io_uring
#
#   # Run epoll server, then:
#   ./bench_uring_vs_epoll.sh epoll
#
#   # Run io_uring server, then:
#   ./bench_uring_vs_epoll.sh iouring
set -euo pipefail

MODE="${1:-epoll}"
HOST="${BENCH_HOST:-127.0.0.1}"
PORT="${BENCH_PORT:-8080}"
THREADS="${BENCH_THREADS:-16}"
CONNECTIONS="${BENCH_CONNECTIONS:-1024}"
DURATION="${BENCH_DURATION:-10s}"
URL="http://${HOST}:${PORT}/plaintext"
OUTDIR="bench_results"
mkdir -p "$OUTDIR"

if ! command -v wrk &>/dev/null; then
    echo "Error: wrk not found. Install it first (https://github.com/wg/wrk)."
    exit 1
fi

echo "=== Chopin benchmark: ${MODE} ==="
echo "URL:         ${URL}"
echo "Threads:     ${THREADS}"
echo "Connections: ${CONNECTIONS}"
echo "Duration:    ${DURATION}"
echo ""

OUTFILE="${OUTDIR}/bench_${MODE}_$(date +%Y%m%d_%H%M%S).txt"

# Warmup run (3s)
echo "Warming up..."
wrk -t"${THREADS}" -c"${CONNECTIONS}" -d3s "${URL}" >/dev/null 2>&1 || true

# Actual run
echo "Running benchmark..."
wrk -t"${THREADS}" -c"${CONNECTIONS}" -d"${DURATION}" --latency "${URL}" | tee "${OUTFILE}"

echo ""
echo "Results saved to: ${OUTFILE}"
echo ""
echo "To compare: diff bench_results/bench_epoll_*.txt bench_results/bench_iouring_*.txt"
