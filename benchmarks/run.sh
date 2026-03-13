#!/usr/bin/env bash
# Chopin HTTP framework benchmark runner.
#
# Runs wrk against a live Chopin server and saves timestamped results.
#
# Prerequisites:
#   - wrk  (brew install wrk)
#   - A running Chopin server (see below)
#
# Quick start:
#   # Terminal 1 — start server (epoll, port 8080):
#   cargo run --release --example bench_chopin -p chopin-core
#
#   # Terminal 1 — start server (io_uring, port 8080):
#   CHOPIN_IO_URING=1 cargo run --release --example bench_chopin -p chopin-core
#
#   # Terminal 2 — run this script:
#   ./benchmarks/run.sh [epoll|iouring]
#
# Environment overrides:
#   BENCH_HOST        default: 127.0.0.1
#   BENCH_PORT        default: 8080
#   BENCH_THREADS     default: $(nproc) on Linux, sysctl on macOS
#   BENCH_CONNECTIONS default: 512
#   BENCH_DURATION    default: 30s
#   BENCH_WARMUP      default: 5s
set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------
MODE="${1:-epoll}"
HOST="${BENCH_HOST:-127.0.0.1}"
PORT="${BENCH_PORT:-8080}"
# Auto-detect logical CPU count
if command -v nproc &>/dev/null; then
    CPU_COUNT=$(nproc)
elif command -v sysctl &>/dev/null; then
    CPU_COUNT=$(sysctl -n hw.logicalcpu 2>/dev/null || echo 8)
else
    CPU_COUNT=8
fi
THREADS="${BENCH_THREADS:-$CPU_COUNT}"
CONNECTIONS="${BENCH_CONNECTIONS:-512}"
DURATION="${BENCH_DURATION:-30s}"
WARMUP="${BENCH_WARMUP:-5s}"
URL="http://${HOST}:${PORT}/plaintext"
RESULTS_DIR="$(cd "$(dirname "$0")" && pwd)/results"
mkdir -p "$RESULTS_DIR"

# ---------------------------------------------------------------------------
# Dependency check
# ---------------------------------------------------------------------------
if ! command -v wrk &>/dev/null; then
    echo "Error: wrk not found."
    echo "  macOS:  brew install wrk"
    echo "  Ubuntu: apt install wrk"
    echo "  Build:  https://github.com/wg/wrk"
    exit 1
fi

# Verify server is up before benchmarking
if ! curl -sf --connect-timeout 2 "${URL}" >/dev/null 2>&1; then
    echo "Error: No server responding at ${URL}"
    echo ""
    echo "Start a server first:"
    echo "  cargo run --release --example bench_chopin -p chopin-core"
    exit 1
fi

# ---------------------------------------------------------------------------
# Run benchmark
# ---------------------------------------------------------------------------
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
OUTFILE="${RESULTS_DIR}/bench_${MODE}_${TIMESTAMP}.txt"

echo "=== Chopin Benchmark: ${MODE} ==="
echo "URL:         ${URL}"
echo "Threads:     ${THREADS}"
echo "Connections: ${CONNECTIONS}"
echo "Duration:    ${DURATION}"
echo "Warmup:      ${WARMUP}"
echo ""

echo "Warming up (${WARMUP})..."
wrk -t"${THREADS}" -c"${CONNECTIONS}" -d"${WARMUP}" "${URL}" >/dev/null 2>&1 || true
sleep 1

echo "Running benchmark (${DURATION})..."
wrk -t"${THREADS}" -c"${CONNECTIONS}" -d"${DURATION}" --latency "${URL}" | tee "${OUTFILE}"

echo ""
echo "Results saved: ${OUTFILE}"
