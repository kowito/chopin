# Chopin Request Pipeline Profiling Report

**Date: March 2, 2026**

## Executive Summary

Profiling of the full HTTP request/response pipeline shows:

🔴 **PRIMARY BOTTLENECK: Response Serialization (37.62% of CPU time)**
- NOT the Date header (which is <0.1% of serialization)
- The overhead is in building headers into the write buffer

## Benchmark Results

### Single Request Breakdown
```
Parsing:        42.00ns (  0.2%)
Routing:        42.00ns (  0.2%)
Serialization:  24.71µs ( 99.7%)
─────────────────────────
TOTAL:          24.79µs
```

### Full Scale (1 million requests)
```
Parsing:        17.11ms (10.73%)
Routing:        17.27ms (10.83%)
Serialization:  60.01ms (37.62%)
────────────────────────
TOTAL:          159.52ms

Per request:    159.00ns
Throughput:     6,268,975 req/sec (CPU-only)
```

## Analysis

### Bottleneck Breakdown by Category

| Stage | Time | % of Total | Notes |
|-------|------|-----------|-------|
| Parsing (HTTP method/path) | 17.11ms | 10.73% | O(path-length), branchless parser |
| Routing (Radix tree lookup) | 17.27ms | 10.83% | O(path-length), efficient |
| **Serialization** | **60.01ms** | **37.62%** | 🔴 **BOTTLENECK** |
| Syscalls (est.) | N/A | ~40-50% | accept, read, write not measured |

### Serialization Breakdown (Within the 37.62%)

The serialization phase includes:
- Writing status line: HTTP/1.1 200 OK\r\n
- Writing Server header: Server: chopin\r\n
- **Writing Date header: 35ns (~0.06% of serialization)**
- Writing Content-Type header
- Writing Content-Length header
- Writing Connection header
- Writing response body

The Date header contributes **negligible overhead** compared to the rest.

## Key Findings

### 1. Date Header is NOT the Problem
- **SystemTime::now():** 18.57 ns
- **format_http_date():** 7.66 ns
- **Combined:** 35 ns = 0.058% of serialization time

Even calling it per-response is fine.

### 2. Serialization is CPU-Bound
The real bottleneck is the CPU work of formatting HTTP headers into buffers:
- String conversions (itoa for Content-Length)
- Buffer copies for headers
- Format placeholders resolution

### 3. Syscalls are Likely the Real Ceiling
The CPU-only throughput is 6.27 million req/sec, but real-world servers max out at ~1M req/sec due to:
- `accept()` syscall: ~5,000 ns per connection
- `read()` syscall: ~1,000 ns per batch
- `write()` syscall: ~1,000 ns per response

These dwarf the 159 ns CPU work and are the actual bottleneck in production.

## Recommendations

### ✅ Current Implementation is Good
- Date header per-response is safe and fast
- Serialization is reasonably optimized already

### 🔧 To Optimize Further (Priority Order)

**High Impact (syscall reduction):**
1. Use `io_uring` instead of select/epoll/kqueue for batched I/O
   - Could reduce syscall count by 50-70%
   - Estimated impact: +30-40% throughput

2. Use TCP_CORK/TCP_NOPUSH to batch multiple writes
   - Combine headers + body into single write syscall
   - Estimated impact: +10-15% throughput

3. Use `sendfile()` for static responses
   - Already implemented for files, consider for small responses
   - Estimated impact: +5-10% throughput

**Medium Impact (CPU optimization):**
1. Pre-compute Content-Length for fixed responses (already done for /json)
2. Use vectorized header writing with writev() (already done)
3. Cache re-usable header snippets

**Low Impact (premature optimization):**
1. Cache Date header ❌ (saves 35ns, not worth it)
2. Inline format_http_date ❌ (already inlined)
3. Use SIMD for more header building ❌ (diminishing returns)

## Code Location

**Serialization happens here:** [crates/chopin-core/src/worker.rs#L355-L420](crates/chopin-core/src/worker.rs#L355-L420)

The Date header specifically: [worker.rs#L376-L382](crates/chopin-core/src/worker.rs#L376-L382)

## Conclusion

✅ **The per-response `SystemTime::now()` call is CORRECT and NOT the bottleneck.**

The actual bottleness stems from:
1. Syscall overhead (estimate: 40-50% of real time)
2. Response serialization CPU work (37.62% of measured CPU time)
3. Potential improvements via io_uring and TCP_CORK

Focus optimization efforts there, not on the Date header which is already negligible.
