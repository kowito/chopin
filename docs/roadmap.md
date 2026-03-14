# Roadmap: Making Chopin as Fast as ntex

## Executive Summary

**ntex** (by fafhrd91, the original Actix-web author) is one of the fastest Rust web frameworks. After deep analysis of both codebases, here is what makes ntex fast and what chopin can adopt.

---

## Why ntex Is Fast

### 1. Buffer Pool with Reuse (`ntex-bytes`)

ntex uses a **buffer pool** system where `BytesMut` buffers are returned to a pool on release instead of being deallocated. This eliminates repeated `malloc`/`free` cycles for read/write buffers across requests.

- `BufConfig.release(buf)` returns buffers to a thread-local pool
- `BufConfig.get()` retrieves a pre-allocated buffer from the pool
- `BufConfig.resize()` / `resize_min()` grow buffers intelligently based on configured high/low watermarks

**Chopin today:** Allocates fixed 8 KiB read + 16 KiB write buffers per `Conn` in the slab. No pooling for dynamically-sized response buffers (`Vec`, `BytesMut`).

### 2. Unsafe `copy_nonoverlapping` Header Encoding

ntex encodes HTTP headers using raw `ptr::copy_nonoverlapping` with a pre-calculated position counter, advancing a raw pointer through the output buffer. This avoids per-header bounds checks and formatting overhead.

- Tracks `remaining` capacity and `pos` offset manually
- Only calls `advance_mut()` / `resize_min()` when capacity runs out
- Single `unsafe { dst.advance_mut(pos) }` at the end to commit all writes

**Chopin today:** Uses `memcpy`-based writes to `write_buf` with bounds checks per header and `format!`-style integer serialization.

### 3. Pre-baked Content-Length with Lookup Table (`DEC_DIGITS_LUT`)

ntex uses a 200-byte digit lookup table for fast integer-to-ASCII conversion in `write_content_length()` and `write_status_line()`. Dedicated code paths handle 1-digit, 2-digit, and 3-digit content-lengths with pre-baked `\r\ncontent-length: ` prefix arrays.

**Chopin today:** Has fast-path `FAST_200_*` constants but uses a custom reverse-order loop for Content-Length which is slower than LUT-based conversion.

### 4. Layered I/O Filter Architecture (`ntex-io`)

ntex's I/O layer uses a **filter stack** (like middleware for bytes). Each filter layer has its own read/write buffer pair. This enables TLS, compression, and logging to be inserted transparently without copying data between layers.

- `Stack` struct with `INLINE_SIZE=3` layers (stack-allocated for common case)
- `FilterCtx` provides zero-overhead access to read/write buffers at each layer
- Source/destination buffer pattern enables zero-copy between filter layers

**Chopin today:** No filter/middleware I/O layer. TLS and compression would require custom integration.

### 5. Configurable Buffer Sizing with Watermarks

ntex allows per-connection buffer configuration with high/low watermarks:
- `set_read_buf(high, low, initial)` — auto-resize based on traffic patterns
- `set_write_buf(high, low, initial)` — backpressure when write buffer exceeds high watermark
- Default `BUFFER_SIZE = 32_768` (32 KiB) vs chopin's 8 KiB read / 16 KiB write

**Chopin today:** Fixed buffer sizes baked into `Conn` struct. No adaptive sizing.

### 6. Async Pipeline with Backpressure and Concurrency

ntex's dispatcher uses a `PipelineBinding` that allows **concurrent in-flight requests** on a single connection. When the service handler is slow, ntex spawns the current future and continues reading/decoding more requests.

- `call_nowait()` — fire-and-forget service call, doesn't block the read loop
- `inflight` counter tracks concurrent requests per connection
- Write backpressure pauses reads when output buffer is full
- `poll_read_pause()` pauses read-side when service isn't ready

**Chopin today:** Processes requests sequentially per connection. Pipelining reads multiple requests but processes them one at a time.

### 7. Multi-Runtime Support (Tokio, Compio, Neon, io_uring)

ntex abstracts the runtime via `ntex-net` and `ntex-rt`, supporting:
- **Tokio** — standard async runtime
- **Compio** — completion-based I/O (Windows IOCP, Linux io_uring)
- **Neon** — ntex's own lightweight runtime with optional io_uring
- **Neon-polling** — cross-platform polling fallback

**Chopin today:** Direct `epoll`/`kqueue` syscalls with experimental `io_uring` behind feature flag. No runtime abstraction.

### 8. Efficient Date Header Caching (`DateService`)

ntex caches the Date header value and updates it periodically (every second), avoiding per-response `httpdate` formatting.

**Chopin today:** Currently no date header caching (explicitly deferred).

---

## Comparison Matrix

| Feature | ntex | Chopin | Gap |
|---------|------|--------|-----|
| Buffer pooling | Thread-local pool with reuse | Fixed slab buffers | **High** |
| Read buffer default | 32 KiB | 8 KiB | **Medium** |
| Write buffer default | 32 KiB | 16 KiB | **Low** |
| Header encoding | Unsafe ptr copy + LUT | memcpy + custom loop | **Medium** |
| Content-Length encoding | LUT-based, pre-baked arrays | Reverse-order loop | **Low** |
| Concurrent requests/conn | Yes (spawn + inflight) | Sequential only | **High** |
| Write backpressure | Yes (pause reads) | Partial (flush before pipeline) | **Medium** |
| I/O filter layers | Yes (stack of filters) | None | **Medium** |
| Adaptive buffer sizing | High/low watermarks | Fixed | **Medium** |
| Date header caching | Per-second cache | Per-response generation | **Low** |
| io_uring | Via neon-uring + compio | Feature-flagged, partial | **Medium** |
| Runtime abstraction | Multi-runtime | Direct syscalls | **Low** (chopin's choice) |
| Thread model | Multi-runtime (can be thread-per-core) | Thread-per-core only | Parity (design choice) |
| Router | Regex-based (`ntex-router`) | Trie + O(1) fast-table | **Chopin wins** |
| Slab allocator | None (standard alloc) | Pre-allocated 25K slots | **Chopin wins** |
| Zero-copy sendfile | No | Yes (platform-optimized) | **Chopin wins** |
| SIMD parsing | httparse (no SIMD) | memchr (SIMD-accelerated) | **Chopin wins** |

---

## Implementation Roadmap

### Phase 1: Buffer Optimization (Highest Impact)

**Goal:** Reduce allocation pressure and increase buffer throughput.

#### 1.1 — Increase Default Buffer Sizes
- **File:** `crates/chopin-core/src/conn.rs`
- **Change:** Increase `read_buf` from 8 KiB → 32 KiB, `write_buf` from 16 KiB → 32 KiB
- **Impact:** Fewer syscalls for larger requests/responses, better pipelining
- **Tradeoff:** Memory per connection increases from ~24 KiB to ~64 KiB (25K slots = 1.6 GB vs 600 MB)
- **Recommendation:** Make configurable; default 32 KiB, allow tuning down for high-connection-count deployments

#### 1.2 — BytesMut Buffer Pool for Dynamic Allocations
- **New file:** `crates/chopin-core/src/bufpool.rs`
- **Design:** Thread-local free-list of `Vec<u8>` buffers (per worker, no synchronization needed)
- **Use for:** JSON serialization buffers, response body construction, chunked encoding
- **Reference:** ntex's `BufConfig.get()` / `BufConfig.release()` pattern

#### 1.3 — Adaptive Buffer Watermarks
- **File:** `crates/chopin-core/src/conn.rs`, `crates/chopin-core/src/worker.rs`
- **Design:** Track read/write utilization per connection; grow/shrink buffers based on high/low watermarks
- **Defer if:** Fixed buffers prove sufficient in benchmarks

**Verification:**
- Benchmark with `wrk -t4 -c256 -d10s` before and after
- Memory profiling with `heaptrack` to confirm reduced allocations

---

### Phase 2: Encoding Optimization (Medium Impact)

**Goal:** Match ntex's header encoding speed.

#### 2.1 — LUT-based Integer Encoding
- **File:** `crates/chopin-core/src/worker.rs` (response serialization section)
- **Change:** Replace custom reverse-order Content-Length formatting with `DEC_DIGITS_LUT` approach
- **Design:** 200-byte static lookup table, special-cased paths for 1/2/3-digit lengths
- **Reference:** ntex's `write_content_length()` in `encoder.rs`

#### 2.2 — Unsafe Header Serialization with Raw Pointers
- **File:** `crates/chopin-core/src/worker.rs`
- **Change:** Batch header writes using `copy_nonoverlapping` with position tracking
- **Design:** Pre-calculate total header size, reserve once, write through raw pointer, advance once
- **Impact:** Eliminates per-header bounds checks (~5-10 ns per header saved)

#### 2.3 — Pre-baked Status Line Constants
- **File:** `crates/chopin-core/src/http.rs`
- **Change:** Add `STATUS_LINE_*` byte arrays for common status codes (200, 201, 204, 301, 302, 400, 404, 500)
- **Impact:** Single memcpy for status line instead of formatting

**Verification:**
- Micro-benchmark header encoding in isolation (`criterion`)
- TFB-style benchmark comparing responses/sec

---

### Phase 3: Concurrent Request Processing (High Impact)

**Goal:** Process multiple requests concurrently on a single connection.

#### 3.1 — In-Flight Request Counter
- **File:** `crates/chopin-core/src/conn.rs`
- **Change:** Add `inflight: u8` field tracking concurrent request handlers
- **Design:** Increment on dispatch, decrement on response write completion

#### 3.2 — Async-Style Request Dispatch (Design Decision Required)
- **Option A:** Stay synchronous but pre-parse multiple pipelined requests and batch-dispatch
- **Option B:** Add lightweight task spawning for handlers (requires mini executor per worker)
- **Option C:** Allow configurable concurrency limit per connection (1 = current behavior)
- **Recommendation:** Option A for simplicity, defer B/C unless benchmarks demand it

#### 3.3 — Write Backpressure
- **File:** `crates/chopin-core/src/worker.rs`
- **Change:** When `write_buf` utilization exceeds 75%, pause read processing
- **Design:** Check write buffer capacity before calling `read_nonblocking()`
- **Impact:** Prevents unbounded buffering under load

**Verification:**
- Load test with slow clients (`wrk -c10000 -t4 --latency`)
- Verify no OOM under backpressure scenarios

---

### Phase 4: Date Header Caching (Low-Effort Win)

**Goal:** Eliminate per-response date generation overhead.

#### 4.1 — Per-Worker Date Cache
- **File:** `crates/chopin-core/src/worker.rs`
- **Change:** Cache formatted Date header in a `[u8; 29]` array, refresh every ~1000ms
- **Design:** Check timestamp at start of event loop iteration; regenerate if second has changed
- **Impact:** Saves ~20 ns per response x millions of requests = measurable improvement
- **Note:** Project has previously deferred this; reconsider for benchmarks

**Verification:**
- Before/after with TFB plaintext benchmark

---

### Phase 5: io_uring Completion (Medium Impact, Linux-only)

**Goal:** Finish the io_uring backend to production quality.

#### 5.1 — Fixed Buffers Registration
- **File:** `crates/chopin-core/src/syscalls.rs` (`uring` module)
- **Change:** Register slab buffers with `io_uring_register_buffers()` for zero-copy reads
- **Impact:** Eliminates kernel-to-userspace buffer copies on read

#### 5.2 — io_uring Sendfile
- **File:** `crates/chopin-core/src/worker_uring.rs`
- **Change:** Use `IORING_OP_SPLICE` for file serving instead of fallback `sendfile_nonblocking()`
- **Impact:** Full completion-based I/O path for static files

#### 5.3 — SQPOLL Mode
- **File:** `crates/chopin-core/src/syscalls.rs`
- **Change:** Enable `IORING_SETUP_SQPOLL` with configurable idle timeout
- **Impact:** Eliminates `submit_and_wait()` syscall overhead (kernel thread polls SQ)

#### 5.4 — Benchmarking & Tuning
- Compare epoll vs io_uring on identical Linux hardware
- Tune ring sizes, SQ depth, and batch sizes

**Verification:**
- Linux benchmark comparing epoll vs io_uring workers
- Latency histograms with `wrk --latency`

---

### Phase 6: I/O Filter Architecture (Long-term)

**Goal:** Enable transparent TLS and compression without data copying.

#### 6.1 — Filter Trait Design
- **New file:** `crates/chopin-core/src/filter.rs`
- **Design:** `trait Filter { fn process_read(&self, buf: &mut ReadBuf); fn process_write(&self, buf: &mut WriteBuf); }`
- **Reference:** ntex's `ntex-io` filter stack pattern

#### 6.2 — TLS Filter Implementation
- **New file:** `crates/chopin-core/src/tls.rs`
- **Design:** Wrap rustls/openssl as a filter layer
- **Impact:** Enables HTTPS without forking the entire connection handler

**Note:** This is a significant architectural change. Defer unless TLS support becomes a priority.

---

## Priority Summary

| Phase | Impact | Effort | Priority |
|-------|--------|--------|----------|
| 1: Buffer Optimization | High | Medium | **P0 — Do First** |
| 2: Encoding Optimization | Medium | Low | **P1 — Quick Wins** |
| 3: Concurrent Requests | High | High | **P1 — Design Needed** |
| 4: Date Header Cache | Low | Low | **P2 — Easy Win** |
| 5: io_uring Completion | Medium | Medium | **P2 — Linux-only** |
| 6: I/O Filters | Medium | High | **P3 — Long-term** |

---

## What Chopin Already Does Better Than ntex

1. **SIMD-accelerated parsing** — `memchr` gives 10-20x faster delimiter search vs `httparse`
2. **Pre-allocated slab** — Zero allocation on hot path, ntex uses standard allocator
3. **O(1) static route fast-table** — ntex uses regex-based router, slower for static routes
4. **Zero-copy sendfile** — Platform-optimized kernel file serving
5. **Thread-per-core with SO_REUSEPORT** — No cross-thread contention at all
6. **Fixed-size header arrays** — Stack-allocated, no per-request header map allocation
7. **writev scatter-gather** — Headers + body in single syscall for large responses
8. **Body::Raw bypass** — Pre-baked responses skip all serialization

These advantages should be preserved while adopting ntex's buffer management and encoding techniques.