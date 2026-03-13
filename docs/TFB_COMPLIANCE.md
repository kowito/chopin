# TechEmpower Benchmark Compliance Guide

Chopin is designed for extreme performance, often achieving numbers that might look "too good to be true." To ensure your Chopin implementation is classified as **Realistic** (and not **Stripped**) by the TechEmpower Framework Benchmarks (TFB) project, follow the guidelines in this document.

---

## 1. What is "Realistic" vs "Stripped"?

In TFB rules, an implementation is marked as **Stripped** if it cuts corners that a production-grade framework wouldn't. This includes:
- Hardcoding the entire response (including headers) into a single byte buffer.
- Skipping mandatory HTTP headers (Date, Server).
- Bypassing the framework's routing logic for specific benchmark endpoints.
- Hand-rolling JSON strings instead of using a serializer.

Chopin provides high-performance paths for all of these that are **100% compliant** with the "Realistic" classification.

---

## 2. Rule-by-Rule Compliance in Chopin

### A. HTTP Headers (Date & Server)
*   **The Rule**: Every response must include `Server` and `Date` headers. The `Date` must be accurate (rendering once per second is an acceptable optimization).
*   **Chopin Compliance**: Chopin automatically generates a compliant `Server: chopin` header and a dynamic `Date` header for every response. We use a high-performance `httpdate` implementation that adds only ~20ns of overhead — far faster than the 1s caching optimization allowed by TFB.
*   **How to stay Realistic**: Use standard response builders (`Response::json`, `Response::text`). Avoid `Response::raw(&'static [u8])` in TFB submissions, as it requires you to hardcode the headers yourself, which can be flagged as "Stripped."

### B. Request Routing
*   **The Rule**: Requests must be routed via a framework-managed router or a standard library router.
*   **Chopin Compliance**: Use the `#[get("/path")]` attribute macro. Chopin's router is a professional-grade Radix Tree/FastPath hybrid.
*   **How to stay Realistic**: Do not write custom `match` logic inside the worker or connection loops to identify `/plaintext`. Use the standard `Chopin::mount_all_routes()` startup sequence.

### C. JSON Serialization
*   **The Rule**: Serialization must occur within the scope of each request and must use a real JSON serializer. Caching pre-rendered JSON fragments is forbidden.
*   **Chopin Compliance**: Use `kowito-json` via `#[derive(KJson)]`.
*   **How to stay Realistic**:
    ```rust
    #[derive(KJson)]
    struct Message {
        message: &'static str,
    }

    #[get("/json")]
    fn json(_ctx: Context) -> Response {
        // instantiation + serialization happens per request
        Response::json(&Message { message: "Hello, World!" })
    }
    ```
    This is fully compliant because `Message` is instantiated inside the handler, and `val.serialize(&mut buf)` is called every time.

### D. Single/Multiple Queries (ORM vs Raw)
*   **The Rule**: The "ORM" test category requires every row to be converted to an object using an ORM tool. Individual queries must be sent separately.
*   **Chopin Compliance**: Use `chopin-orm`. 
*   **How to stay Realistic**: Use the `Model` trait and `FromRow` derive macros. Chopin-ORM uses "index-based extraction" (e.g., `row.get(0)`), which is a zero-cost abstraction used by production-grade Rust ORMs like Diesel and SQLx. It is fully permitted.

---

## 3. Recommended TFB Pattern

To achieve 10M+ req/s on Plaintext while remaining "Realistic," use the following patterns:

### Plaintext (Test #6)
```rust
#[get("/plaintext")]
fn plaintext(_ctx: Context) -> Response {
    // Realistic: headers are dynamic, body is a static slice.
    Response::text_static(b"Hello, World!")
}
```

### JSON (Test #1)
```rust
#[get("/json")]
fn json(_ctx: Context) -> Response {
    // Realistic: Object instantiation + SIMD serialization per request.
    Response::json(&Message { message: "Hello, World!" })
}
```

### Database (Test #2)
```rust
#[get("/db")]
fn db(_ctx: Context) -> Response {
    let id = rand_id();
    // Realistic: DB Pool checkout + single query + ORM mapping.
    let row = conn.query_one("SELECT id, randomnumber FROM world WHERE id = $1", &[&id])?;
    Response::json(&World::from_row(row))
}
```

---

## 4. Optimization Checklists

| Optimization | Realistic | Stripped | Status in Chopin |
| :--- | :---: | :---: | :--- |
| io_uring backend | ✅ | | Standard Linux feature |
| SIMD JSON (kowito-json) | ✅ | | Standard Rust library |
| Pipelined I/O Batching | ✅ | | Standard server optimization |
| Dynamic Date Headers | ✅ | | Standard framework behavior |
| Hardcoded Status Lines | | ❌ | **Avoid `Response::raw`** |
| Manual TCP `write` bypassing router | | ❌ | **Always use `#[get]`** |

---

## Summary
The goal of Chopin is to provide the **fastest possible realistic framework**. By using our standard macros (`#[get]`) and builders (`Response::json`), you get all the performance of a handwritten C server while keeping the code clean, maintainable, and fully compliant with all TechEmpower rules.
