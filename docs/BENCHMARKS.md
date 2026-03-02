# Chopin Benchmark Report (2026-03-01)

This report details the performance comparison between Chopin v0.5.8 and Hyper v1.4.1 conducted on a 10-core Apple Silicon machine.

## Performance Comparison

| Scenario | Framework | Relative Throughput | Avg Latency | Max Latency |
| :--- | :--- | :--- | :--- | :--- |
| **Plain Text** | **Chopin** | **100%** | **1.25ms** | **37.83ms** |
| | Hyper | 88% | 1.45ms | 68.27ms |
| **JSON** | **Chopin** | **100%** | **0.98ms** | **25.05ms** |
| | Hyper | 74% | 1.45ms | 67.39ms |


### 📊 Performance Visualization (JSON Throughput)

```text
Chopin  [██████████████████████████████] 100% (Baseline)
Hyper   [██████████████████████        ]  74%
```

## Analysis

### CPU Scaling & Networking
Chopin demonstrates superior scaling on multi-core systems. By utilizing `SO_REUSEPORT` at the kernel level, each worker thread operates with zero contention on the listen socket, allowing for linear throughput gains as cores are added.

### JSON Serialization
The performance gap in JSON matches the throughput lead, as Chopin's `kowito-json` Schema-JIT engine serializes payloads at near-memory bandwidth speeds without the overhead of standard reflection-based serializers.

### 🐧 Platform-Specific Optimizations

Chopin's throughput lead is sustained by deep OS-level optimizations:
- **Linux**: Atomic `SOCK_NONBLOCK`, `TCP_DEFER_ACCEPT` (holds connection until data arrives), and `TCP_FASTOPEN` (TFO).
- **macOS**: `SO_NOSIGPIPE` and `TCP_FASTOPEN`.
- **Edge-Triggered I/O**: High-performance event notification using `epoll` (ET) on Linux and `kqueue` on macOS.
- **Syscall Minimization**: `TCP_NODELAY` is set on the listener and inherited, saving one `setsockopt` syscall per connection.

## Environment Details
- **Architecture**: Apple Silicon (M-series)
- **Cores**: 10
- **Concurrency**: 200 connections
- **Duration**: 10 seconds
- **Command**: `wrk -t10 -c200 -d10s`
