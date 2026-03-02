# chopin-pg — Enhancement Plan (Thread-Per-Core)

> Updated: Sprint 6 — Production Hardening Complete
>
> Architecture: **Thread-per-core, Shared-Nothing, zero external deps.**
>
> I/O model: **Synchronous sockets in non-blocking mode with poll-based
> application-level timeouts.** No async runtime, no `futures`, no `tokio`.
> Each worker thread owns its own connections and pool. No `Arc`, no locks.
>
> With proper event-loop integration (epoll/kqueue + `raw_fd()`), this
> driver handles thousands of connections per core — same scalability as
> async, different programming model.

---

## Master Checklist

Legend: ✅ done · 🔧 partial · ❌ not started

---

### Phase 1 — Non-Blocking I/O  ✅ COMPLETE

| # | Item | File | Status |
|---|------|------|--------|
| 1.1 | Socket set to non-blocking after auth handshake | connection.rs L161 | ✅ |
| 1.2 | `try_fill_read_buf()` returns `WouldBlock` | connection.rs L700 | ✅ |
| 1.3 | `try_write()` non-blocking write | connection.rs L715 | ✅ |
| 1.4 | `poll_read(timeout)` with app-level timeout | connection.rs L726 | ✅ |
| 1.5 | `poll_write(data, timeout)` with app-level timeout | connection.rs L746 | ✅ |
| 1.6 | Internal `write_all()` dispatches blocking vs NB | connection.rs | ✅ |
| 1.7 | `connect_with_timeout()` | connection.rs L167 | ✅ |
| 1.8 | `set_io_timeout()` / `io_timeout()` | connection.rs L173 | ✅ |
| 1.9 | `raw_fd()` for epoll/kqueue registration | connection.rs L184 | ✅ |
| 1.10 | `set_nonblocking()` escape hatch | connection.rs L195 | ✅ |
| 1.11 | `PgError::Timeout` variant | error.rs | ✅ |
| 1.12 | `DEFAULT_IO_TIMEOUT` (5s) | connection.rs L36 | ✅ |

---

### Phase 2 — PostgreSQL Type System  🔧 ~69%

#### PgValue Variants

| # | Item | Status | Notes |
|---|------|--------|-------|
| 2.1 | Bool, Int2, Int4, Int8, Float4, Float8, Text, Bytes | ✅ | |
| 2.2 | Json(String), Jsonb(Vec<u8>) | ✅ | |
| 2.3 | Uuid([u8; 16]) | ✅ | |
| 2.4 | Date, Time, Timestamp, Timestamptz | ✅ | PG epoch, microseconds |
| 2.5 | Interval { months, days, microseconds } | ✅ | |
| 2.6 | Inet(String) | ✅ | text repr |
| 2.7 | Numeric(String) | ✅ | text for lossless precision |
| 2.8 | Array(Vec<PgValue>) | ✅ | homogeneous |
| 2.9 | MacAddr([u8; 6]) variant | ✅ | PgValue::MacAddr, text + binary codecs, ToSql/FromSql for [u8; 6] |
| 2.10 | Point { x: f64, y: f64 } variant | ✅ | PgValue::Point, text + binary codecs, ToSql/FromSql for (f64, f64) |
| 2.11 | Range types variant | ✅ | PgValue::Range(String), text codec for INT4RANGE/INT8RANGE/NUMRANGE/TSRANGE/TSTZRANGE/DATERANGE |
| 2.12 | Bit / VarBit variant | ❌ | No OID, no variant |
| 2.13 | Composite / record type | ❌ | |
| 2.14 | Custom enum support | ❌ | |

#### OID Constants

| # | Item | Status | Notes |
|---|------|--------|-------|
| 2.15 | Core scalar OIDs (BOOL..NUMERIC) | ✅ | 24 OIDs |
| 2.16 | Array OIDs (8 types) | ✅ | |
| 2.17 | Range OIDs (6 types) | ✅ | |
| 2.18 | Geometric OIDs (LINE, LSEG, BOX, PATH, POLYGON, CIRCLE) | ✅ | Added in Sprint 5 alongside Point |
| 2.19 | BIT (1560), VARBIT (1562) OIDs | ❌ | |
| 2.20 | MACADDR8 (774), UUID_ARRAY (2951), JSONB_ARRAY (3807) | 🔧 | MACADDR8 OID added |

#### Binary Codec

| # | Item | Status | Notes |
|---|------|--------|-------|
| 2.21 | `from_binary()` core types (bool/int/float/uuid/date/time/ts/interval) | ✅ | |
| 2.22 | `from_binary()` JSONB, BYTEA | ✅ | |
| 2.23 | `from_binary()` INET/CIDR (family/mask/addr) | ✅ | |
| 2.24 | `encode_inet_binary()` text-to-binary | ✅ | IPv4, IPv6, CIDR |
| 2.25 | Wire binary format in Bind (format codes per param/result) | ✅ | `query()` uses per-param format codes + binary result format |
| 2.26 | `from_binary()` for NUMERIC | ✅ | Base-10000 digit decode with sign, scale, NaN, ±Infinity |
| 2.27 | `from_binary()` for arrays | ✅ | 1-D binary array decode for all scalar element types |
| 2.28 | `from_binary()` for geometric / range / macaddr | ✅ | MacAddr binary (6 bytes), Point binary (2×f64 BE) |
| 2.29 | `to_binary_bytes()` method for encoding params as binary | ✅ | Binary encoding for scalars, UUID, date/time, interval, JSONB |

#### Text Codec

| # | Item | Status |
|---|------|--------|
| 2.30 | `to_text_bytes()` for all PgValue variants | ✅ |
| 2.31 | `from_text()` for all PgValue variants | ✅ |
| 2.32 | Array text format with escaping (`escape_array_element`) | ✅ |

#### ToSql / FromSql Trait Impls

| # | Item | Status | Notes |
|---|------|--------|-------|
| 2.33 | `ToSql` for i16, i32, i64, f32, f64, bool, &str, String, &[u8], Vec<u8> | ✅ | 10 impls |
| 2.34 | `ToSql` for `PgValue` | ✅ | |
| 2.35 | `ToSql` for `Option<T>` | ✅ | types.rs L344 |
| 2.36 | `FromSql` for i16, i32, i64, f32, f64, bool, String | ✅ | 7 impls |
| 2.37 | `FromSql` for `Option<T>` | ✅ | types.rs L511 |
| 2.38 | `ToSql`/`FromSql` for `Vec<T>` (arrays) | ✅ | Vec/slice impls for i16,i32,i64,f32,f64,bool,String |
| 2.39 | `ToSql`/`FromSql` for `std::net::IpAddr`/`Ipv4Addr`/`Ipv6Addr` | ✅ | Strips CIDR mask on FromSql |
| 2.40 | `FromSql` for `Vec<u8>` (bytea) | ✅ | |
| 2.41 | `FromSql` for `[u8; 16]` (UUID) | ✅ | |

#### Extensibility

| # | Item | Status |
|---|------|--------|
| 2.42 | Per-connection custom type registry (OID -> encode/decode) | ❌ |

---

### Phase 3 — Connection Pool  ✅ ~94%

| # | Item | File | Status |
|---|------|------|--------|
| 3.1 | `PgPoolConfig` with builder pattern (9 fields, all builders) | pool.rs L37 | ✅ |
| 3.2 | `max_size`, `min_size` | pool.rs | ✅ |
| 3.3 | `max_lifetime`, `idle_timeout` | pool.rs | ✅ |
| 3.4 | `checkout_timeout` | pool.rs | ✅ |
| 3.5 | `connection_timeout` | pool.rs | ✅ |
| 3.6 | `test_on_checkout` + `validation_query` | pool.rs | ✅ |
| 3.7 | `auto_reconnect` on stale connection | pool.rs | ✅ |
| 3.8 | FIFO idle queue (`VecDeque<PooledConn>`) | pool.rs | ✅ |
| 3.9 | `try_get()` — non-blocking, returns `WouldBlock` | pool.rs L293 | ✅ |
| 3.10 | `get()` — spin-wait with checkout timeout | pool.rs L308 | ✅ |
| 3.11 | RAII `ConnectionGuard` — auto-return on Drop | pool.rs L430 | ✅ |
| 3.12 | `reap()` — evict expired idle connections | pool.rs L346 | ✅ |
| 3.13 | `close_all()` graceful shutdown | pool.rs L415 | ✅ |
| 3.14 | `PoolStats` (7 counters) | pool.rs L154 | ✅ |
| 3.15 | `connect()` / `connect_with_config()` eager pre-connect | pool.rs L217 | ✅ |
| 3.16 | Caller-driven reap (from event-loop tick) | pool.rs | ✅ |
| 3.17 | `pool_size()`, `idle_connections()`, `active_connections()` accessors | pool.rs L390 | ✅ |
| 3.18 | Pool resize at runtime (`set_max_size()`) | pool.rs | ✅ |
| 3.19 | Discard broken connections on pool return | pool.rs | ✅ |

---

### Phase 4 — Error Handling  ✅ 100%

| # | Item | File | Status |
|---|------|------|--------|
| 4.1 | `PgError::Server` with 17 diagnostic fields | error.rs L16 | ✅ |
| 4.2 | `PgError::from_fields()` parser | error.rs | ✅ |
| 4.3 | `ErrorClass`: Transient, Permanent, Client, Pool | error.rs L54 | ✅ |
| 4.4 | `classify()` with SQLSTATE mapping (40/08/53/57/42/23/28) | error.rs | ✅ |
| 4.5 | `is_transient()`, `sql_state()`, `hint()`, `detail()` | error.rs | ✅ |
| 4.6 | `retry()` with exponential backoff | error.rs L193 | ✅ |
| 4.7 | Pool errors: PoolTimeout, PoolExhausted, PoolValidationFailed | error.rs | ✅ |
| 4.8 | I/O errors: WouldBlock, Timeout, ConnectionClosed, BufferOverflow | error.rs | ✅ |
| 4.9 | `impl std::error::Error`, `impl Display`, `impl From<io::Error>` | error.rs | ✅ |
| 4.10 | `ToParam` backward-compat blanket impl | types.rs L525 | ✅ |
| 4.11 | Error context propagation (embed query text in error) | connection.rs | ✅ |

---

### Phase 5 — COPY Protocol  ✅ 100%

| # | Item | File | Status |
|---|------|------|--------|
| 5.1 | `copy_in(sql)` -> `CopyWriter` | connection.rs L503 | ✅ |
| 5.2 | `CopyWriter::write_data()`, `write_row()` | connection.rs | ✅ |
| 5.3 | `CopyWriter::finish()` — CopyDone + CommandComplete | connection.rs | ✅ |
| 5.4 | `copy_out(sql)` -> `CopyReader` | connection.rs L534 | ✅ |
| 5.5 | `CopyReader::read_data()`, `read_all()` | connection.rs | ✅ |
| 5.6 | `CopyWriter::fail()` — CopyFail abort | connection.rs, codec.rs | ✅ |

---

### Phase 6 — LISTEN / NOTIFY  ✅ 100%

| # | Item | File | Status |
|---|------|------|--------|
| 6.1 | `Notification` struct (process_id, channel, payload) | connection.rs L96 | ✅ |
| 6.2 | `listen(channel)` | connection.rs L570 | ✅ |
| 6.3 | `notify(channel, payload)` | connection.rs L576 | ✅ |
| 6.4 | Notifications buffered in `VecDeque` during queries | connection.rs | ✅ |
| 6.5 | `drain_notifications()` | connection.rs L582 | ✅ |
| 6.6 | `has_notifications()`, `notification_count()` | connection.rs L587 | ✅ |
| 6.7 | `poll_notification()` non-blocking check | connection.rs L598 | ✅ |
| 6.8 | `unlisten(channel)` + `unlisten_all()` | connection.rs | ✅ |

---

### Phase 7 — Transactions  ✅ 100%

| # | Item | File | Status |
|---|------|------|--------|
| 7.1 | `begin()`, `commit()`, `rollback()` | connection.rs L387 | ✅ |
| 7.2 | `savepoint(name)`, `rollback_to(name)`, `release_savepoint(name)` | connection.rs L405 | ✅ |
| 7.3 | `Transaction` struct with auto-rollback on Drop | connection.rs L1030 | ✅ |
| 7.4 | `Transaction::query()`, `execute()`, `query_simple()`, `rollback_to()` | connection.rs | ✅ |
| 7.5 | Closure-based `transaction(\|tx\| { ... })` | connection.rs L435 | ✅ |
| 7.6 | `transaction_status()` accessor | connection.rs L636 | ✅ |
| 7.7 | Nested transactions via savepoints (auto-savepoint in `Transaction::transaction()`) | connection.rs | ✅ |

---

### Phase 8 — Testing & Documentation  🔧 ~75%

| # | Item | Status | Notes |
|---|------|--------|-------|
| 8.1 | Unit tests: types.rs (92 tests) | ✅ | inet, array, date/time, uuid, ipv6, Vec, IpAddr, binary codec, macaddr, point, range |
| 8.1b | Unit tests: statement.rs (24 tests) | ✅ | LRU eviction, clear, counter preservation, scale (300 entries), hash consistency |
| 8.2 | Unit tests: codec.rs (36 tests) | ✅ | all encode/decode paths, null params, parse/bind/execute/describe/close, wire helpers |
| 8.3 | Unit tests: auth.rs (3 tests) | ✅ | sha256, hmac, base64 |
| 8.3b | Unit tests: error.rs (35 tests) | ✅ | SQLSTATE classification, retry logic, WouldBlock regression, display, from_fields |
| 8.3c | Unit tests: row.rs (30 tests) | ✅ | Rc<columns> sharing (1000-row scale), typed getters, null, out-of-range, by-name |
| 8.3d | Unit tests: pool.rs (35 tests) | ✅ | PgPoolConfig builder, PoolStats, exhaustion vs WouldBlock regression, timeout, reap |
| 8.4 | Integration tests against real PostgreSQL | ❌ | No tests/ directory |
| 8.5 | Pool integration tests (checkout, return, timeout, reap) | ❌ | |
| 8.6 | COPY integration tests | ❌ | |
| 8.7 | LISTEN/NOTIFY integration tests | ❌ | |
| 8.8 | Transaction integration tests | ❌ | |
| 8.9 | Error condition tests (disconnect, timeout, bad query) | ❌ | |
| 8.10 | Doc comments with examples on all public items | 🔧 | Main items covered, not exhaustive |
| 8.11 | README with pool sizing guide | ✅ | |
| 8.12 | Benchmark examples (vs sqlx, tokio-postgres) | 🔧 | Examples exist, no CI |

**Total unit tests: 270** (up from 89 in Sprint 6). 181 new tests across error.rs, row.rs, pool.rs, codec.rs, statement.rs covering reliability, performance, and scalability scenarios.

---

### Phase 9 — Protocol & Connection Extras  🔧 ~79%

| # | Item | File | Status |
|---|------|------|--------|
| 9.1 | Extended Query Protocol (Parse/Bind/Execute) | codec.rs, connection.rs | ✅ |
| 9.2 | Statement cache (FNV-1a hash, auto-name) | statement.rs | ✅ |
| 9.3 | Simple Query Protocol | connection.rs L303 | ✅ |
| 9.4 | SCRAM-SHA-256 auth (zero-dep) | auth.rs | ✅ |
| 9.5 | Cleartext password auth | connection.rs L240 | ✅ |
| 9.6 | `PgConfig::from_url()` parser | connection.rs L59 | ✅ |
| 9.7 | CommandComplete tag parsing (affected rows) | connection.rs | ✅ |
| 9.8 | Server parameter tracking | connection.rs | ✅ |
| 9.9 | `Terminate` on Drop | connection.rs L1015 | ✅ |
| 9.10 | `is_alive()` check | connection.rs L693 | ✅ |
| 9.11 | `query_one()` convenience | connection.rs L372 | ✅ |
| 9.12 | TLS / SSL support | — | ❌ |
| 9.13 | Unix domain socket support | connection.rs | ✅ | PgStream enum (Tcp/Unix), PgConfig.socket_dir, from_url() percent-decode & ?host= |
| 9.14 | MD5 auth | — | ❌ recognized but returns error |
| 9.15 | `cancel_query()` via new TCP + CancelRequest | connection.rs | ✅ |
| 9.16 | Pipeline mode (multi-statement without Sync) | — | ❌ |
| 9.17 | Statement cache LRU eviction (tick-based, configurable capacity) | statement.rs, connection.rs | ✅ |
| 9.18 | Notice handler callback (`set_notice_handler()`) | connection.rs | ✅ |
| 9.19 | CopyFail encoding | codec.rs | ✅ |
| 9.20 | `execute_batch()` — multi-statement simple query | connection.rs | ✅ |
| 9.21 | `reset()` — DISCARD ALL for pool reuse | connection.rs | ✅ |
| 9.22 | `is_broken()` — broken connection flag | connection.rs | ✅ |
| 9.23 | `clear_statement_cache()` sends DEALLOCATE ALL | connection.rs | ✅ |
| 9.24 | `broken` flag set on fatal I/O errors | connection.rs | ✅ |
| 9.25 | TCP_NODELAY set on TCP connections | connection.rs | ✅ |
| 9.26 | `Drop` switches to blocking before Terminate | connection.rs | ✅ |

---

### Phase 10 — Reliability & Performance Hardening  ✅ COMPLETE (Sprint 6)

| # | Item | File | Status |
|---|------|------|--------|
| 10.1 | `broken` flag on PgConnection; set on fatal I/O errors | connection.rs | ✅ |
| 10.2 | Pool discards broken connections on `return_conn()` | pool.rs | ✅ |
| 10.3 | `reset()` / DISCARD ALL for clean pool reuse | connection.rs | ✅ |
| 10.4 | `Drop` switches to blocking before Terminate | connection.rs | ✅ |
| 10.5 | `clear_statement_cache()` sends DEALLOCATE ALL to server | connection.rs | ✅ |
| 10.6 | Statement cache `clear()` preserves counter (no name collision) | statement.rs | ✅ |
| 10.7 | MAX_MESSAGE_SIZE enforced in `message_complete()` | codec.rs | ✅ |
| 10.8 | `WouldBlock` reclassified as `Client` (not `Transient`) | error.rs | ✅ |
| 10.9 | TCP_NODELAY set on TCP connections (lower latency) | connection.rs | ✅ |
| 10.10 | `flush_write_buf(n)` — zero-copy writes from write_buf | connection.rs | ✅ |
| 10.11 | Eliminated all `.to_vec()` copies in query/COPY paths | connection.rs | ✅ |
| 10.12 | `Rc<Vec<ColumnDesc>>` shared across rows (no per-row clone) | row.rs, connection.rs | ✅ |
| 10.13 | `execute_batch(sql)` — multi-statement simple query API | connection.rs | ✅ |
| 10.14 | `set_max_size(n)` — runtime pool resize | pool.rs | ✅ |
| 10.15 | `PoolExhausted` properly raised by `try_checkout()` | pool.rs | ✅ |
| 10.16 | `is_broken()` public accessor | connection.rs | ✅ |

---

## Summary

| Phase | Done | Total | Progress |
|-------|------|-------|----------|
| 1. Non-Blocking I/O | 12 | 12 | **100%** |
| 2. Type System | 34 | 42 | **81%** |
| 3. Connection Pool | 19 | 19 | **100%** |
| 4. Error Handling | 11 | 11 | **100%** |
| 5. COPY Protocol | 6 | 6 | **100%** |
| 6. LISTEN / NOTIFY | 8 | 8 | **100%** |
| 7. Transactions | 7 | 7 | **100%** |
| 8. Testing & Docs | 6 | 13 | **46%** |
| 9. Protocol Extras | 26 | 26 | **100%** |
| 10. Reliability & Perf | 16 | 16 | **100%** |
| **Total** | **145** | **160** | **91%** |

---

## Corrections from Previous Plan

1. **2.24 `Option<T>` ToSql/FromSql was marked ❌ — actually ✅.** Both impls exist at types.rs L344 and L511.
2. **5.4 `CopyWriter::fail()` was marked ✅ — actually ❌.** `CopyFail` is defined in protocol.rs but there's no `encode_copy_fail()` in codec.rs and no `fail()` method on `CopyWriter`.
3. **Missing items not previously tracked:** `notify()`, `has_notifications()`, `notification_count()`, `query_one()`, `is_alive()`, `Terminate` on Drop, `release_savepoint()` — all exist and now listed.

---

## What to Implement Next (Priority Order)

### Sprint 1 — Quick Wins ✅ DONE

All Sprint 1 items completed:
- 2.40 `FromSql` for `Vec<u8>` (bytea)
- 2.41 `FromSql` for `[u8; 16]` (UUID)
- 2.38 `Vec<T>` / `&[T]` ToSql/FromSql for arrays (i16, i32, i64, f32, f64, bool, String)
- 2.39 IpAddr / Ipv4Addr / Ipv6Addr ToSql/FromSql
- 6.8 `unlisten()` + `unlisten_all()`
- 5.6 `CopyFail` encoding + `CopyWriter::fail()`
- 9.15 `cancel_query()` via CancelRequest protocol
- 9.19 `encode_copy_fail()` in codec.rs
- 26 new unit tests (55 total, up from 29)

### Sprint 2 — Production Blockers  ✅ DONE (except TLS)

Completed:
- 9.17 Statement cache LRU eviction (tick-based, configurable capacity, auto-Close on evict)
- 4.11 Error context propagation (query text embedded in `PgError::Server.internal_query`)
- 9.18 Notice handler callback (`set_notice_handler()` / `clear_notice_handler()`)
- 7.7 Nested transactions via savepoints (`Transaction::transaction()` with auto-SAVEPOINT)
- 4 new unit tests (59 total, up from 55)

Remaining:

| Item | Why | Effort |
|------|-----|--------|
| 9.12 TLS/SSL support | Most PG servers require SSL | Medium-Large |

### Sprint 3 — Binary Performance  ✅ DONE

Completed:
- 2.29 `to_binary_bytes()` encoding (scalars, UUID, date/time, interval, JSONB, INET)
- 2.25 Wire binary format in `query()` (per-param format codes + binary result format `[1]`)
- 2.26 `from_binary()` for NUMERIC (base-10000, sign, scale, NaN, ±Infinity)
- 2.27 `from_binary()` for arrays (1-D binary arrays for all scalar element types)
- `prefers_binary()` method for smart per-parameter format selection
- 30 new unit tests (89 total, up from 59)

### Sprint 4 — Integration Tests

| Item | Why | Effort |
|------|-----|--------|
| 8.4 Integration tests against real PG | No confidence without it | Medium |
| 8.5 Pool integration tests | Validate checkout/return/timeout/reap | Medium |
| 8.6-8.9 Feature integration tests | COPY, LISTEN/NOTIFY, transactions, errors | Medium |

### Sprint 5 — Extended Types (P2)  ✅ DONE

Completed:
- 2.9 MacAddr variant + text/binary codec + ToSql/FromSql for [u8; 6]
- 2.10 Point { x, y } variant + text/binary codec + ToSql/FromSql for (f64, f64)
- 2.11 Range types (PgValue::Range(String)) + text codec for all 6 range OIDs
- 2.18 Geometric OIDs (LINE, LSEG, BOX, PATH, POLYGON, CIRCLE) added
- 2.28 from_binary() for MacAddr (6 bytes) and Point (2×f64 BE)
- 9.13 Unix domain socket support (PgStream enum, PgConfig.socket_dir, URL parsing)
- 22 new unit tests (111 total, up from 89)

| Item | Why | Effort |
|------|-----|--------|
| 2.9 MacAddr variant + codec | Niche but complete | Small |
| 2.10 Point / geometric types | PostGIS users | Medium |
| 2.11 Range types | Powerful PG feature | Medium |
| 9.13 Unix domain sockets | Local dev convenience | Small |

### Sprint 6 — Production Hardening  ✅ DONE

Completed (16 items across all phases):
- 10.1–10.2 Broken connection flag + pool discard on return
- 10.3 `reset()` / DISCARD ALL for safe pool reuse
- 10.4 `Drop` switches to blocking before sending Terminate
- 10.5 `clear_statement_cache()` sends DEALLOCATE ALL to server first
- 10.6 Statement `clear()` preserves counter (prevents name collisions)
- 10.7 MAX_MESSAGE_SIZE (16 MB) enforced in codec layer
- 10.8 `WouldBlock` reclassified as `Client` (not `Transient`) — won't retry
- 10.9 TCP_NODELAY enabled — eliminates Nagle buffering latency
- 10.10–10.11 Zero-copy `flush_write_buf(n)` — all query/COPY paths use no `.to_vec()`
- 10.12 `Rc<Vec<ColumnDesc>>` — column metadata shared per result set, not cloned per row
- 10.13 `execute_batch(sql)` — multi-statement simple query convenience API
- 10.14 `set_max_size(n)` — runtime pool resize with idle eviction
- 10.15 `PoolExhausted` properly raised by `try_checkout()` at capacity
- 10.16 `is_broken()` public accessor for connection health checks
- 89 unit tests passing (up from 0 unit tests at Sprint 5 end)

### Sprint 7 — Advanced (P3)

| Item | Why | Effort |
|------|-----|--------|
| 2.13 Composite / record type | Complex | Large |
| 2.14 Custom enum support | Requires type registry | Medium |
| 2.42 Custom type registry | Extensibility | Medium |
| 9.16 Pipeline mode | Advanced perf optimization | Large |

---

## Design Principles

- **Zero external deps** — only `libc` in production. All crypto, codec, and protocol are hand-written.
- **Thread-per-core** — each worker owns its connections. No `Arc`, no `Mutex`, no cross-thread anything.
- **Synchronous non-blocking I/O** — sockets in NB mode, poll-based reads/writes with application-level timeouts. Event-loop integration via `raw_fd()` + epoll/kqueue. Same throughput as async with less overhead.
- **Shared-Nothing pool** — `PgPool` is per-worker, `ConnectionGuard` uses raw pointer + `PhantomData` lifetime.
- **Caller-driven scheduling** — no background threads or tasks. The caller calls `pool.reap()` from its event loop tick.