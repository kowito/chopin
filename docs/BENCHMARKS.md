# Chopin Benchmark Report (2026-03-01)

This report details the performance comparison between Chopin v0.5.2 and Hyper v1.4.1 conducted on a 10-core Apple Silicon machine.

## Performance Comparison

| Scenario | Framework | Requests/sec | Avg Latency | Max Latency |
| :--- | :--- | :--- | :--- | :--- |
| **Plain Text** | **Chopin** | **183,196** | **1.25ms** | **37.83ms** |
| | Hyper | 161,197 | 1.45ms | 68.27ms |
| **JSON** | **Chopin** | **217,357** | **0.98ms** | **25.05ms** |
| | Hyper | 161,463 | 1.45ms | 67.39ms |

## Analysis

### CPU Scaling & Networking
Chopin demonstrates superior scaling on multi-core systems. By utilizing `SO_REUSEPORT` at the kernel level, each worker thread operates with zero contention on the listen socket, allowing for linear throughput gains as cores are added.

### JSON Serialization
The performance gap in JSON matches the throughput lead, as Chopin's `kowito-json` Schema-JIT engine serializes payloads at near-memory bandwidth speeds without the overhead of standard reflection-based serializers.

## Environment Details
- **Architecture**: Apple Silicon (M-series)
- **Cores**: 10
- **Concurrency**: 200 connections
- **Duration**: 10 seconds
- **Command**: `wrk -t10 -c200 -d10s`
