#!/bin/bash
# Profile the TFB server to identify bottlenecks

set -e

BINARY="./target/release/examples/tfb"
PORT=8000
PROFILE_DURATION=10
LOAD_DURATION=30

# Kill any existing server
pkill -f "target/release/examples/tfb" || true
sleep 1

# Start server in background
echo "Starting server..."
$BINARY &
SERVER_PID=$!
sleep 2

# Verify server is responding
echo "Testing server..."
curl -s http://localhost:$PORT/json | head -c 50
echo ""

# Start profiling with macOS `sample` command
echo "Starting CPU profiling for $PROFILE_DURATION seconds..."
PROFILE_FILE="profile_$(date +%s).txt"

# Run profile in background
sample "$SERVER_PID" -file "$PROFILE_FILE" -mayDie 2>/dev/null &
SAMPLE_PID=$!
sleep 1

# Generate load with curl in parallel
echo "Generating load..."
load_gen() {
  for i in {1..100}; do
    curl -s http://localhost:$PORT/json &
    curl -s http://localhost:$PORT/plaintext &
  done | while read line; do :; done
}

# Run load generation in background for LOAD_DURATION
for i in $(seq 1 $LOAD_DURATION); do
  echo "  Load gen iteration $i/$LOAD_DURATION"
  load_gen &
  sleep 1
done

wait

# Stop profiling
sleep 3
kill $SAMPLE_PID 2>/dev/null || true
kill $SERVER_PID 2>/dev/null || true

# Show results
echo ""
echo "========================================"
echo "Profile saved to: $PROFILE_FILE"
echo "========================================"
echo ""
echo "Top functions by sample count:"
echo "========================================" 
grep "^[0-9]" "$PROFILE_FILE" | head -30 || cat "$PROFILE_FILE" | head -50

echo ""
echo "Full profile at: $PROFILE_FILE"
