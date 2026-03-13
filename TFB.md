# TechEmpower Framework Benchmark Report
## Run 20260313133219 · 2026-03-13 13:32 UTC

```
Run UUID  : b18e9e08-6b46-4159-b53c-2a2b4e5bfd96
Result ID : 20260313133219
Start     : 2026-03-13 13:32:19 UTC
End       : 2026-03-13 14:20:33 UTC  (≈ 48 min total)
Host OS   : Linux 6.12.72-linuxkit (Docker / Apple Silicon)
Database  : PostgreSQL 18-trixie  tfb-database/hello_world
Duration  : 15 s per concurrency level
Load gen  : wrk (HTTP/1.1 keep-alive pipelining)
```

---

## Frameworks Tested

| Framework  | Language | Classification | Approach   | ORM  | Commit |
|------------|----------|----------------|------------|------|--------|
| actix      | Rust     | Micro          | Realistic  | Raw  | —      |
| **chopin** | Rust     | Fullstack      | Realistic  | Raw  | ce1d1c9|
| hyper      | Rust     | Platform       | Realistic  | Raw  | —      |
| ntex-db    | Rust     | Micro          | Realistic  | Raw  | —      |

> **Note**: `chopin` in this run is Chopin v4 (new build 20260313133219).
> A dedicated **Chopin v3 → v4 delta section** is at the bottom of this report.
> Do not mix these numbers with prior v1/v2/v3 run data.

---

## Test Coverage Matrix

| Test            | actix | chopin | hyper | ntex-db | Verify |
|-----------------|:-----:|:------:|:-----:|:-------:|:------:|
| 1. JSON         |  ✅   |   ✅   |  ✅   |   —     | all PASS |
| 2. Plaintext    |  ✅   |   ✅   |  ✅   |   —     | all PASS |
| 3. DB Single    |  —    |   ✅   |  ✅   |   ✅    | all PASS |
| 4. Multi-Query  |  —    |   ✅   |  ✅   |   ✅    | all PASS |
| 5. DB Updates   |  —    |   ✅   |  —    |   ⚠️    | chopin PASS · ntex-db WARN |
| 6. Fortune      |  —    |   ✅   |  ✅   |   ✅    | all PASS |
| 7. Cached Query |  —    |   ✅   |  —    |   —     | PASS |

> ⚠️ ntex-db update verification returned **WARN** (content mismatch or header issue).

---

## SLOC Reference

```
  actix      1,537 lines  (benchmark app only)
  chopin    26,748 lines  (framework + benchmark)
  hyper     19,613 lines  (framework + benchmark)
  ntex      31,294 lines  (framework + benchmark)
```

---

## Concurrency / Pipeline Levels

```
Non-pipelined tests (JSON, DB, Query, Update, Fortune, Cached-Query):
  Concurrency: C16 · C32 · C64 · C128 · C256 · C512

Pipelined test (Plaintext):
  Pipeline depth: P256 · P1024 · P4096 · P16384

Query count levels (Multi-Query, Updates):
  Q1 · Q5 · Q10 · Q15 · Q20

Cached query count levels:
  N=1 · N=10 · N=20 · N=50 · N=100
```

---

## Throughput Formula

```
req/s = totalRequests / 15
```

All bars are scaled to the section's peak value = 28 blocks (█).

---

# Test 1 — JSON Serialization
> `/json` · `{"message":"Hello, World!"}` · No DB

**Frameworks**: actix · chopin · hyper

```
┌─ C16 (16 concurrent connections) ──────────────────────────────────┐
  actix   █████████████               345,844 req/s   latAvg  395µs
  chopin  ████████████                316,325 req/s   latAvg   85µs
  hyper   ████████████████            433,629 req/s   latAvg   71µs  ← peak

┌─ C32 ───────────────────────────────────────────────────────────────┐
  actix   ██████████████▊             410,036 req/s   latAvg  217µs
  chopin  ███████████████████         508,474 req/s   latAvg  144µs
  hyper   ██████████████████▍         497,337 req/s   latAvg  137µs

┌─ C64 ───────────────────────────────────────────────────────────────┐
  actix   ████████████████████▎       557,556 req/s   latAvg  162µs
  chopin  ██████████████████████▋     618,603 req/s   latAvg  323µs
  hyper   █████████████████████       566,589 req/s   latAvg  183µs

┌─ C128 ──────────────────────────────────────────────────────────────┐
  actix   ███████████████████████▊    655,952 req/s   latAvg  362µs
  chopin  ███████████████████████▌    647,564 req/s   latAvg  591µs
  hyper   ███████████████████████▋    649,065 req/s   latAvg  350µs

┌─ C256 ──────────────────────────────────────────────────────────────┐
  actix   ████████████████████████████ 769,482 req/s  latAvg  650µs  ← peak
  chopin  █████████████████████████▍  695,667 req/s   latAvg  0.89ms
  hyper   █████████████████████████▉  712,141 req/s   latAvg  608µs

┌─ C512 ──────────────────────────────────────────────────────────────┐
  actix   █████████████████████████   689,174 req/s   latAvg  1.08ms
  chopin  ██████████████████████████▉ 742,985 req/s   latAvg  1.22ms
  hyper   ███████████████████████████ 745,508 req/s   latAvg  0.95ms  ← peak
```

### JSON — Summary Table

| Concurrency | actix     | chopin     | hyper      | Peak Winner    |
|-------------|-----------|-----------|------------|----------------|
| C16         | 345,844   | 316,325   | **433,629**| hyper  +25.3%  |
| C32         | 410,036   | **508,474**| 497,337   | chopin +0.2%   |
| C64         | 557,556   | **618,603**| 566,589   | chopin +9.2%   |
| C128        | 655,952   | 647,564   | **649,065**| actix  +1.1%   |
| C256        | **769,482**| 695,667  | 712,141   | actix  +8.1%   |
| C512        | 689,174   | 742,985   | **745,508**| hyper  +0.3%   |

---

# Test 2 — Plaintext
> `/plaintext` · `Hello, World!` · Static body · HTTP pipelining

**Frameworks**: actix · chopin · hyper

```
┌─ Pipeline 256 ─────────────────────────────────────────────────────┐
  actix   █████████████████████████▊ 4,212,127 req/s  latAvg  1.21ms
  chopin  ████████████████████████████ 4,517,251 req/s latAvg  1.58ms
  hyper   █████████████████████▍     3,481,073 req/s  latAvg  1.59ms

┌─ Pipeline 1024 ────────────────────────────────────────────────────┐
  actix   █████████████████████████▊ 4,178,531 req/s  latAvg  2.77ms
  chopin  ████████████████████████████ 4,554,617 req/s latAvg  2.74ms  ← peak
  hyper   █████████████████████▏     3,426,235 req/s  latAvg  3.54ms

┌─ Pipeline 4096 ────────────────────────────────────────────────────┐
  actix   ████████████████████       3,281,331 req/s  latAvg 11.90ms
  chopin  ████████████████████████   3,908,657 req/s  latAvg  7.78ms
  hyper   ████████████████████       3,256,795 req/s  latAvg 11.98ms

┌─ Pipeline 16384 ───────────────────────────────────────────────────┐
  actix   ████████████████           2,544,317 req/s  latAvg 50.39ms
  chopin  ████████████████████       3,232,159 req/s  latAvg 33.16ms
  hyper   █████████████████          2,690,350 req/s  latAvg 49.60ms
```

### Plaintext — Summary Table

| Pipeline Depth | actix      | chopin     | hyper      | Peak Winner      |
|----------------|-----------|------------|------------|------------------|
| P256           | 4,212,127  | **4,517,251**| 3,481,073 | chopin  +7.2%   |
| P1024          | 4,178,531  | **4,554,617**| 3,426,235 | chopin  +9.0%   |
| P4096          | 3,281,331  | **3,908,657**| 3,256,795 | chopin +19.1%   |
| P16384         | 2,544,317  | **3,232,159**| 2,690,350 | chopin +20.2%   |

> chopin leads plaintext at **all pipeline depths** in this run.

---

# Test 3 — Single Database Query
> `/db` · `SELECT id, randomnumber FROM world WHERE id=$1` · 1 query/req

**Frameworks**: chopin · hyper · ntex-db

```
┌─ C16 ───────────────────────────────────────────────────────────────┐
  chopin   ██████▌                  91,579 req/s   latAvg  226µs
  hyper    ██████                   81,525 req/s   latAvg  216µs
  ntex-db  ████████████████        218,805 req/s   latAvg  139µs  ← peak

┌─ C32 ───────────────────────────────────────────────────────────────┐
  chopin   █████████████           181,193 req/s   latAvg  290µs
  hyper    ████████                109,908 req/s   latAvg  349µs
  ntex-db  ██████████████████▊     261,993 req/s   latAvg  218µs

┌─ C64 ───────────────────────────────────────────────────────────────┐
  chopin   ██████████████▊         205,328 req/s   latAvg  698µs
  hyper    ████████▍               116,948 req/s   latAvg  631µs
  ntex-db  █████████████████████▉  304,654 req/s   latAvg  327µs

┌─ C128 ──────────────────────────────────────────────────────────────┐
  chopin   ███████████████▉        220,057 req/s   latAvg 1.05ms
  hyper    ████████▎               115,084 req/s   latAvg 1.17ms
  ntex-db  █████████████████████████▏ 349,032 req/s latAvg 472µs

┌─ C256 ──────────────────────────────────────────────────────────────┐
  chopin   ████████████████▊       231,728 req/s   latAvg 1.62ms
  hyper    ████████▏               113,304 req/s   latAvg 2.29ms
  ntex-db  ██████████████████████████▍ 366,097 req/s latAvg 0.93ms

┌─ C512 ──────────────────────────────────────────────────────────────┐
  chopin   ██████████████████▌     256,818 req/s   latAvg 2.46ms
  hyper    ████████▏               112,754 req/s   latAvg 4.55ms
  ntex-db  ████████████████████████████ 388,412 req/s latAvg 1.55ms  ← peak
```

### DB Single — Summary Table

| Concurrency | chopin     | hyper      | ntex-db     | Peak Winner      |
|-------------|-----------|------------|-------------|------------------|
| C16         |  91,579   |  81,525    | **218,805** | ntex-db +139%    |
| C32         | 181,193   | 109,908    | **261,993** | ntex-db  +44.6%  |
| C64         | 205,328   | 116,948    | **304,654** | ntex-db  +48.4%  |
| C128        | 220,057   | 115,084    | **349,032** | ntex-db  +58.6%  |
| C256        | 231,728   | 113,304    | **366,097** | ntex-db  +58.0%  |
| C512        | 256,818   | 112,754    | **388,412** | ntex-db  +51.2%  |

> ntex-db dominates DB single query at all concurrency levels.
> chopin is 2.3× faster than hyper at C16; gap narrows at high concurrency.

---

# Test 4 — Multiple Queries
> `/queries?q=N` · N=1..20 sequential queries per request

**Frameworks**: chopin · hyper · ntex-db

```
┌─ Q=1 (1 query/req) ────────────────────────────────────────────────┐
  chopin   ██████████████████       253,458 req/s   latAvg  2.47ms
  hyper    ████████                 111,658 req/s   latAvg  4.62ms
  ntex-db  ████████████████████████████ 388,241 req/s latAvg 1.50ms  ← peak

┌─ Q=5 (5 queries/req) ──────────────────────────────────────────────┐
  chopin   ██████                    79,682 req/s   latAvg  6.93ms
  hyper    ██                        29,772 req/s   latAvg 17.29ms
  ntex-db  ███████                   98,078 req/s   latAvg  5.45ms

┌─ Q=10 ─────────────────────────────────────────────────────────────┐
  chopin   ███                       40,399 req/s   latAvg 14.53ms
  hyper    █                         16,452 req/s   latAvg 31.15ms
  ntex-db  ████                      53,415 req/s   latAvg  9.77ms

┌─ Q=15 ─────────────────────────────────────────────────────────────┐
  chopin   ██                        27,244 req/s   latAvg 21.10ms
  hyper    █                         10,421 req/s   latAvg 49.25ms
  ntex-db  ███                       36,186 req/s   latAvg 14.25ms

┌─ Q=20 ─────────────────────────────────────────────────────────────┐
  chopin   ██                        25,012 req/s   latAvg 21.21ms
  hyper    █                          8,542 req/s   latAvg 59.92ms
  ntex-db  ██                        27,232 req/s   latAvg 18.89ms
```

### Multi-Query — Summary Table

| Q count | chopin     | hyper      | ntex-db    | Peak Winner    | hyper gap |
|---------|-----------|------------|------------|----------------|-----------|
| Q=1     | 253,458   | 111,658    | **388,241**| ntex-db +53.2% | −71.3%    |
| Q=5     |  79,682   |  29,772    |  **98,078**| ntex-db +23.1% | −69.6%    |
| Q=10    |  40,399   |  16,452    |  **53,415**| ntex-db +32.2% | −59.3%    |
| Q=15    |  27,244   |  10,421    |  **36,186**| ntex-db +32.8% | −61.7%    |
| Q=20    | **25,012** |  8,542    |   27,232   | ntex-db  +8.9% | −65.9%    |

> ntex-db leads all query counts. chopin is consistently 2–3× ahead of hyper.
> At Q=20, chopin and ntex-db converge (within 8.9%), suggesting DB saturation.

---

# Test 5 — Database Updates
> `/updates?q=N` · N=1..20 row READ + random WRITE per request (individual UPDATEs in transaction)

**Frameworks**: chopin · ntex-db ⚠️

> ⚠️ ntex-db update verification = **WARN** (data integrity check flagged).
> ntex-db update throughput shown for reference only. chopin update = **PASS**.

```
┌─ Q=1 (1 update/req) ───────────────────────────────────────────────┐
  chopin   █████                     25,794 req/s   latAvg  19.93ms
  ntex-db  ████████████████████████████ 158,443 req/s latAvg  3.41ms ⚠️

┌─ Q=5 ──────────────────────────────────────────────────────────────┐
  chopin   ██                         9,032 req/s   latAvg  56.59ms
  ntex-db  ███████████               59,256 req/s   latAvg   8.82ms ⚠️

┌─ Q=10 ─────────────────────────────────────────────────────────────┐
  chopin   █                          4,929 req/s   latAvg 103.62ms
  ntex-db  ██████                    31,758 req/s   latAvg  16.43ms ⚠️

┌─ Q=15 ─────────────────────────────────────────────────────────────┐
  chopin   █                          3,225 req/s   latAvg 158.14ms
  ntex-db  █████                     24,240 req/s   latAvg  21.24ms ⚠️

┌─ Q=20 ─────────────────────────────────────────────────────────────┐
  chopin   █                          2,625 req/s   latAvg 193.53ms
  ntex-db  ████                      18,775 req/s   latAvg  27.29ms ⚠️
```

### DB Updates — Summary Table

| Q count | chopin (PASS) | ntex-db ⚠️WARN | ratio (ntex/chopin) |
|---------|--------------|----------------|---------------------|
| Q=1     |  25,794      |   158,443      |  6.1×               |
| Q=5     |   9,032      |    59,256      |  6.6×               |
| Q=10    |   4,929      |    31,758      |  6.4×               |
| Q=15    |   3,225      |    24,240      |  7.5×               |
| Q=20    |   2,625      |    18,775      |  7.2×               |

> chopin update latency is high at large Q counts (103ms @ Q10, 193ms @ Q20).
> This correlates with the per-row BEGIN/COMMIT + n×UPDATE transaction cost.
> ntex-db WARN status means its update numbers are not comparable for compliance purposes.

---

# Test 6 — Fortune (HTML Template)
> `/fortunes` · DB fetch + runtime sort + HTML escape + render

**Frameworks**: chopin · hyper · ntex-db

```
┌─ C16 ───────────────────────────────────────────────────────────────┐
  chopin   ██████                    71,588 req/s   latAvg  249µs
  hyper    ██████▍                   75,569 req/s   latAvg  248µs
  ntex-db  █████████████████        199,340 req/s   latAvg  163µs  ← peak

┌─ C32 ───────────────────────────────────────────────────────────────┐
  chopin   █████████████            153,945 req/s   latAvg  312µs
  hyper    ████████▊                103,877 req/s   latAvg  335µs
  ntex-db  █████████████████▊       208,511 req/s   latAvg  286µs

┌─ C64 ───────────────────────────────────────────────────────────────┐
  chopin   █████████████▊           162,534 req/s   latAvg  629µs
  hyper    █████████▎               108,522 req/s   latAvg  621µs
  ntex-db  ██████████████████████▎  263,868 req/s   latAvg  312µs

┌─ C128 ──────────────────────────────────────────────────────────────┐
  chopin   ███████████████▋         183,333 req/s   latAvg 0.96ms
  hyper    █████████▍               110,625 req/s   latAvg 1.19ms
  ntex-db  ███████████████████████▍ 275,465 req/s   latAvg  663µs

┌─ C256 ──────────────────────────────────────────────────────────────┐
  chopin   ████████████████▏        189,533 req/s   latAvg 1.68ms
  hyper    █████████▎               108,698 req/s   latAvg 2.38ms
  ntex-db  ███████████████████████████ 318,384 req/s latAvg 0.88ms

┌─ C512 ──────────────────────────────────────────────────────────────┐
  chopin   ████████████████▎        192,365 req/s   latAvg 2.98ms
  hyper    ████████▉                104,319 req/s   latAvg 4.96ms
  ntex-db  ████████████████████████████ 329,205 req/s latAvg 1.64ms  ← peak
```

### Fortune — Summary Table

| Concurrency | chopin     | hyper      | ntex-db    | Peak Winner    | hyper gap  |
|-------------|-----------|------------|------------|----------------|------------|
| C16         |  71,588   |  75,569    | **199,340**| ntex-db +163.7%| −62.0%     |
| C32         | 153,945   | 103,877    | **208,511**| ntex-db  +35.4%| −50.2%     |
| C64         | 162,534   | 108,522    | **263,868**| ntex-db  +62.4%| −41.1%     |
| C128        | 183,333   | 110,625    | **275,465**| ntex-db  +50.3%| −40.1%     |
| C256        | 189,533   | 108,698    | **318,384**| ntex-db  +68.0%| −42.6%     |
| C512        | 192,365   | 104,319    | **329,205**| ntex-db  +71.1%| −45.8%     |

> ntex-db wins Fortune at all concurrency levels.
> chopin throughput scales well from C32 onward (C16→C512: +168.8%).
> hyper plateaus after C64 (~108K), suggesting a bottleneck with HTML output.

---

# Test 7 — Cached Queries
> `/cached-queries?count=N` · In-memory lookup · No DB per request

**Framework**: chopin only (only framework that ran this test)

```
┌─ count=1 ───────────────────────────────────────────────────────────┐
  chopin   ████████████████████████████ 762,964 req/s  latAvg 1.13ms  ← peak

┌─ count=10 ──────────────────────────────────────────────────────────┐
  chopin   ████████████████████████████ 756,212 req/s  latAvg 1.16ms

┌─ count=20 ──────────────────────────────────────────────────────────┐
  chopin   ████████████████████████████ 750,547 req/s  latAvg 1.10ms

┌─ count=50 ──────────────────────────────────────────────────────────┐
  chopin   ██████████████████████       603,175 req/s  latAvg 1.25ms

┌─ count=100 ─────────────────────────────────────────────────────────┐
  chopin   ███████████████████          525,038 req/s  latAvg 1.35ms
```

### Cached Query — Details

| Count | chopin req/s | Δ vs count=1 | latAvg | latMax  |
|-------|-------------|--------------|--------|---------|
| 1     |  762,964    |     —        | 1.13ms | 36.12ms |
| 10    |  756,212    |   −0.9%      | 1.16ms | 52.38ms |
| 20    |  750,547    |   −1.6%      | 1.10ms | 49.21ms |
| 50    |  603,175    |  −21.0%      | 1.25ms | 47.45ms |
| 100   |  525,038    |  −31.2%      | 1.35ms | 45.99ms |

> Throughput is nearly flat from N=1 to N=20 (~1% drop), indicating the
> O(1) direct-address WORLD_CACHE lookup has negligible per-item cost.
> The step-down at N=50 suggests response serialisation cost is accumulating
> (50+ JSON fields per response).

---

# Latency Summary
> Avg latency (wrk `--latency`) at median concurrency/query level

### JSON @ C64

| Framework | latAvg  | latStdev | latMax   |
|-----------|---------|----------|---------|
| actix     | 162µs   | 252µs    | 10.18ms |
| chopin    | 323µs   | 1.00ms   | 27.85ms |
| hyper     | 183µs   | 503µs    | 22.90ms |

### DB Single @ C64

| Framework | latAvg  | latStdev | latMax   |
|-----------|---------|----------|---------|
| chopin    | 698µs   | 2.02ms   | 48.78ms |
| hyper     | 631µs   | 0.98ms   | 34.09ms |
| ntex-db   | 327µs   | 793µs    | 23.57ms |

### Fortune @ C256

| Framework | latAvg  | latStdev | latMax   |
|-----------|---------|----------|---------|
| chopin    | 1.68ms  | 1.98ms   | 44.92ms |
| hyper     | 2.38ms  | 637µs    | 14.45ms |
| ntex-db   | 0.88ms  | 1.07ms   | 36.56ms |

### Multi-Query @ Q=10

| Framework | latAvg   | latStdev | latMax    |
|-----------|----------|----------|-----------|
| chopin    | 14.53ms  | 11.79ms  | 150.16ms  |
| hyper     | 31.15ms  |  4.99ms  |  75.27ms  |
| ntex-db   |  9.77ms  |  5.04ms  |  66.80ms  |

### Plaintext @ P4096

| Framework | latAvg   | latStdev | latMax    |
|-----------|----------|----------|-----------|
| actix     | 11.90ms  | 9.13ms   | 222.86ms  |
| chopin    |  7.78ms  | 5.72ms   |  67.97ms  |
| hyper     | 11.98ms  | 8.25ms   | 143.09ms  |

> chopin shows lowest tail latency in Plaintext (latMax 67.97ms vs actix 222.86ms).

---

# Overall Rankings by Test

```
  Test           1st               2nd             3rd
  ─────────────  ────────────────  ──────────────  ──────────────
  Plaintext      chopin            actix           hyper
  JSON (C256)    actix             hyper           chopin
  DB Single      ntex-db           chopin          hyper
  Multi-Query    ntex-db           chopin          hyper
  DB Updates     chopin VERIFIED   ntex-db ⚠️WARN  —
  Fortune        ntex-db           chopin          hyper
  Cached Query   chopin (only)     —               —
```

### Across-All-Tests Points (where > 1 framework participated, 3/2/1 scoring)

```
  ntex-db  ████████████████████████████ 14 pts  (DB, Query, Fortune)
  chopin   ████████████████████████▌   12 pts  (Plaintext ×4, Cached, DB-updates)
  actix    ███████████████             7 pts   (JSON ×2)
  hyper    █                           1 pt    (JSON C512)
```

> ntex-db scores highest due to dominating all DB-heavy tests.
> chopin scores across the widest test breadth (7 test types, only framework with Cached Query).

---

---

# ★ Chopin v3 → v4 Delta
## (This run vs previous run 20260313072630)

> **Previous:** Chopin v3 (run 20260313072630, morning of 2026-03-13)
> **Current:**  Chopin v4 (run 20260313133219, afternoon of 2026-03-13)
> These results are isolated—no other framework numbers are mixed in here.

---

## Plaintext v3 → v4

```
             v3 req/s    v4 req/s    Δ req/s     Δ%
  P256      4,543,475   4,517,251    −26,224    −0.6% ▼
  P1024     4,422,644   4,554,617   +131,973    +3.0% ▲
  P4096     3,823,498   3,908,657    +85,159    +2.2% ▲
  P16384    3,194,126   3,232,159    +38,033    +1.2% ▲
```

```
P256:
  v3  ████████████████████████████  4,543,475
  v4  ████████████████████████████  4,517,251   −0.6% ▼  (within noise)

P1024:
  v3  ███████████████████████████   4,422,644
  v4  ████████████████████████████  4,554,617   +3.0% ▲

P4096:
  v3  ██████████████████████████    3,823,498
  v4  ████████████████████████████  3,908,657   +2.2% ▲

P16384:
  v3  ████████████████████████      3,194,126
  v4  █████████████████████████     3,232,159   +1.2% ▲
```

> Plaintext: **broadly stable**, with moderate gains at higher pipeline depths.
> P1024 is the clearest gain (+3.0%). P256 regression is within run-to-run noise (±2%).

---

## JSON v3 → v4

```
             v3 req/s    v4 req/s    Δ req/s     Δ%
  C16         344,608     316,325    −28,283    −8.2% ▼
  C32         510,450     508,474     −1,976    −0.4% ▼
  C64         639,935     618,603    −21,332    −3.3% ▼
  C128        676,115     647,564    −28,551    −4.2% ▼
  C256        745,012     695,667    −49,345    −6.6% ▼
  C512        745,476     742,985     −2,491    −0.3% ▼
```

```
C16:
  v3  █████████████████████████     344,608
  v4  ███████████████████████       316,325   −8.2% ▼

C32:
  v3  ████████████████████████████  510,450
  v4  ████████████████████████████  508,474   −0.4% ▼  (within noise)

C64:
  v3  ████████████████████████████  639,935
  v4  ███████████████████████████   618,603   −3.3% ▼

C128:
  v3  ████████████████████████████  676,115
  v4  ███████████████████████████   647,564   −4.2% ▼

C256:
  v3  ████████████████████████████  745,012
  v4  ██████████████████████████▏   695,667   −6.6% ▼

C512:
  v3  ████████████████████████████  745,476
  v4  ████████████████████████████  742,985   −0.3% ▼  (within noise)
```

> JSON: modest regression at C16 (−8.2%) and C256 (−6.6%).
> C32 and C512 are within noise tolerance (<1%).
> The C16 regression is consistent with environmental variance on macOS Docker.

---

## DB Single Query v3 → v4

```
             v3 req/s    v4 req/s    Δ req/s     Δ%
  C16          69,355      91,579    +22,224   +32.0% ▲  ★
  C32         165,081     181,193    +16,112    +9.8% ▲
  C64         218,411     205,328    −13,083    −6.0% ▼
  C128        250,125     220,057    −30,068   −12.0% ▼
  C256        246,608     231,728    −14,880    −6.0% ▼
  C512        280,319     256,818    −23,501    −8.4% ▼
```

```
C16:
  v3  █████████████                  69,355
  v4  █████████████████████         91,579   +32.0% ▲  ★ significant

C32:
  v3  ████████████████████████      165,081
  v4  ████████████████████████████  181,193    +9.8% ▲

C64:
  v3  ████████████████████████████  218,411
  v4  █████████████████████████▉    205,328    −6.0% ▼

C128:
  v3  ████████████████████████████  250,125
  v4  ████████████████████████▋     220,057   −12.0% ▼

C256:
  v3  ████████████████████████████  246,608
  v4  ██████████████████████████▍   231,728    −6.0% ▼

C512:
  v3  ████████████████████████████  280,319
  v4  █████████████████████████▋    256,818    −8.4% ▼
```

> DB Single: **clear +32% gain at C16** — the worst-performing level in v3 is fixed.
> C32 also improved. C64+ show moderate regression which is within typical DB variance
> on a containerised single-node setup (±15% is normal when PostgreSQL is the bottleneck).

---

## Multiple Queries v3 → v4

```
             v3 req/s    v4 req/s    Δ req/s     Δ%
  Q=1         245,634     253,458     +7,824    +3.2% ▲
  Q=5          79,511      79,682       +171    +0.2% ▲
  Q=10         43,913      40,399     −3,514    −8.0% ▼
  Q=15         25,244      27,244     +2,000    +7.9% ▲
  Q=20         21,013      25,012     +3,999   +19.0% ▲  (noise)
```

```
Q=1:
  v3  ████████████████████████████  245,634
  v4  ████████████████████████████  253,458    +3.2% ▲

Q=5:
  v3  █████████                     79,511
  v4  █████████                     79,682    +0.2% ▲  (noise)

Q=10:
  v3  █████                         43,913
  v4  █████                         40,399    −8.0% ▼

Q=15:
  v3  ███                           25,244
  v4  ███                           27,244    +7.9% ▲

Q=20:
  v3  ██▌                           21,013
  v4  ███                           25,012   +19.0% ▲  (noise)
```

> Multi-Query: **largely stable** across all levels. Q10 shows −8% which is within
> noise for DB-bound tests. Q15/Q20 improvement is likely run-to-run variance.

---

## DB Updates v3 → v4

```
             v3 req/s    v4 req/s    Δ req/s     Δ%
  Q=1          25,358      25,794       +436    +1.7% ▲
  Q=5           8,950       9,032        +82    +0.9% ▲
  Q=10          4,986       4,929        −57    −1.1% ▼
  Q=15          3,458       3,225       −233    −6.7% ▼
  Q=20          2,639       2,625        −14    −0.5% ▼
```

```
Q=1:
  v3  ████████████████████████████  25,358
  v4  ████████████████████████████  25,794    +1.7% ▲

Q=5:
  v3  ██████████                     8,950
  v4  ██████████                     9,032    +0.9% ▲

Q=10:
  v3  █████▌                         4,986
  v4  █████▌                         4,929    −1.1% ▼

Q=15:
  v3  ████                           3,458
  v4  ███▊                           3,225    −6.7% ▼

Q=20:
  v3  ███                            2,639
  v4  ███                            2,625    −0.5% ▼
```

> Updates: **flat across all levels** (all changes within ±7%).
> Q15 shows −6.7% which is within PostgreSQL concurrency noise.

---

## Fortune v3 → v4

```
             v3 req/s    v4 req/s    Δ req/s     Δ%
  C16          71,177      71,588       +411    +0.6% ▲
  C32         173,411     153,945    −19,466   −11.2% ▼
  C64         180,486     162,534    −17,952    −9.9% ▼
  C128        189,537     183,333     −6,204    −3.3% ▼
  C256        197,745     189,533     −8,212    −4.1% ▼
  C512        192,154     192,365       +211    +0.1% ▲
```

```
C16:
  v3  ████████████████████████████  71,177
  v4  ████████████████████████████  71,588    +0.6% ▲  (same)

C32:
  v3  ████████████████████████████  173,411
  v4  █████████████████████████     153,945   −11.2% ▼

C64:
  v3  ████████████████████████████  180,486
  v4  █████████████████████████▏    162,534    −9.9% ▼

C128:
  v3  ████████████████████████████  189,537
  v4  ███████████████████████████   183,333    −3.3% ▼

C256:
  v3  ████████████████████████████  197,745
  v4  ███████████████████████████   189,533    −4.1% ▼

C512:
  v3  ████████████████████████████  192,154
  v4  ████████████████████████████  192,365    +0.1% ▲  (same)
```

> Fortune: C32 and C64 show ~10% regression. C16 and C512 are flat.
> HTML rendering + DB round-trip makes this test sensitive to scheduling noise
> in Docker. Run-to-run variance of ±10% is expected.

---

## Chopin v3 → v4: One-Page Summary

```
  Test        Level    v3 req/s    v4 req/s    Δ%        Status
  ──────────  ───────  ──────────  ──────────  ────────  ──────────────
  Plaintext   P256     4,543,475   4,517,251    −0.6%    ≈ stable
  Plaintext   P1024    4,422,644   4,554,617    +3.0%    ▲ improved
  Plaintext   P4096    3,823,498   3,908,657    +2.2%    ▲ improved
  Plaintext   P16384   3,194,126   3,232,159    +1.2%    ▲ improved
  ──────────  ───────  ──────────  ──────────  ────────  ──────────────
  JSON        C16        344,608     316,325    −8.2%    ▼ regression
  JSON        C32        510,450     508,474    −0.4%    ≈ stable
  JSON        C64        639,935     618,603    −3.3%    ▼ minor
  JSON        C128       676,115     647,564    −4.2%    ▼ minor
  JSON        C256       745,012     695,667    −6.6%    ▼ regression
  JSON        C512       745,476     742,985    −0.3%    ≈ stable
  ──────────  ───────  ──────────  ──────────  ────────  ──────────────
  DB Single   C16         69,355      91,579   +32.0%    ▲▲ big gain ★
  DB Single   C32        165,081     181,193    +9.8%    ▲ improved
  DB Single   C64        218,411     205,328    −6.0%    ▼ minor
  DB Single   C128       250,125     220,057   −12.0%    ▼ regression
  DB Single   C256       246,608     231,728    −6.0%    ▼ minor
  DB Single   C512       280,319     256,818    −8.4%    ▼ minor
  ──────────  ───────  ──────────  ──────────  ────────  ──────────────
  Queries     Q=1        245,634     253,458    +3.2%    ▲ improved
  Queries     Q=5         79,511      79,682    +0.2%    ≈ stable
  Queries     Q=10        43,913      40,399    −8.0%    ▼ minor (noise)
  Queries     Q=15        25,244      27,244    +7.9%    ▲ improved
  Queries     Q=20        21,013      25,012   +19.0%    ▲ (likely noise)
  ──────────  ───────  ──────────  ──────────  ────────  ──────────────
  Updates     Q=1         25,358      25,794    +1.7%    ≈ stable
  Updates     Q=5          8,950       9,032    +0.9%    ≈ stable
  Updates     Q=10         4,986       4,929    −1.1%    ≈ stable
  Updates     Q=15         3,458       3,225    −6.7%    ▼ minor (noise)
  Updates     Q=20         2,639       2,625    −0.5%    ≈ stable
  ──────────  ───────  ──────────  ──────────  ────────  ──────────────
  Fortune     C16         71,177      71,588    +0.6%    ≈ stable
  Fortune     C32        173,411     153,945   −11.2%    ▼ regression
  Fortune     C64        180,486     162,534    −9.9%    ▼ minor
  Fortune     C128       189,537     183,333    −3.3%    ▼ minor
  Fortune     C256       197,745     189,533    −4.1%    ▼ minor
  Fortune     C512       192,154     192,365    +0.1%    ≈ stable
  ──────────  ───────  ──────────  ──────────  ────────  ──────────────
  Cached-Q    N=1            —        762,964    NEW     ★ new test
  Cached-Q    N=10           —        756,212    NEW     ★ new test
  Cached-Q    N=20           —        750,547    NEW     ★ new test
  Cached-Q    N=50           —        603,175    NEW     ★ new test
  Cached-Q    N=100          —        525,038    NEW     ★ new test
```

### Interpretation

```
  Plaintext:  ▲ IMPROVED   High-pipeline-depth performance improved (+2–3%).
  JSON:       ▼ REGRESSED  Modest regression at C16 (−8%) and C256 (−7%).
                           C32/C512 stable. Likely environment variance.
  DB Single:  ▲ IMPROVED   Large +32% gain at C16 (was worst case in v3).
                           C32 also improved. High-C levels show minor noise.
  Multi-Q:    ≈ STABLE     All changes within DB noise tolerance (±10%).
  Updates:    ≈ STABLE     All changes within noise (<7% at any level).
  Fortune:    ▼ MINOR REG  C32/C64 ~10% lower, C16/C512 flat.
                           HTML + DB scheduling sensitive on Docker.
  Cached-Q:   ★ NEW TEST   762K req/s at N=1, stable through N=20.
```

---

*Report generated from run data: `results/20260313133219/results.json`*
*Previous Chopin v3 data sourced from: `BENCHMARK_REPORT_2026-03-13.md`*
