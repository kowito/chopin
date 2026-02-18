# 🎹 Chopin

[![Build status](https://github.com/kowito/chopin/actions/workflows/CI.yml/badge.svg?branch=main)](https://github.com/kowito/chopin/actions/workflows/CI.yml)
[![Crates.io](https://img.shields.io/crates/v/chopin-core)](https://crates.io/crates/chopin-core)
[![Downloads](https://img.shields.io/crates/d/chopin-core.svg)](https://crates.io/crates/chopin-core)
[![License](https://img.shields.io/badge/license-WTFPL-blue.svg)](https://github.com/kowito/chopin/blob/main/LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75+-blue.svg)](https://www.rust-lang.org)

> **High-fidelity engineering for the modern virtuoso.**

**Django meets Rust.** Chopin brings Django's "batteries-included" philosophy to high-performance systems programming. Build **modular, type-safe APIs** at **650K+ req/s** with compile-time verification and zero circular dependencies.

```rust
// Type-safe composition with zero circular dependencies
App::new().await?
    .mount_module(AuthModule::new())
    .mount_module(BlogModule::new())
    .mount_module(BillingModule::new())
    .run().await?;
```

```bash
# Get started in 60 seconds
cargo install chopin-cli
chopin new my-api && cd my-api
REUSEPORT=true cargo run --release --features perf
```

---

## 🏆 Why Chopin?

**The ultimate framework for production APIs:**

- **🏆 #1 Performance** — 657K req/s, 612µs avg latency, 3.75ms p99 ([see benchmarks](docs/BENCHMARKS.md))
- **🎁 All the tools** — Auth, RBAC, database, OpenAPI, caching, file uploads ([explore features](docs/FEATURES.md))
- **💰 50% cost savings** — 3 servers handle what takes 10+ with Node.js/Python
- **🧩 Modular architecture** — Django-style feature composition with Rust safety
- **⚡ Zero-alloc hot paths** — Thread-local buffers, per-route optimization, SO_REUSEPORT
- **📦 Framework included** — No integration hell, no missing packages at 3 AM

---

## 🚀 Get Started

**[→ QUICK START GUIDE](docs/QUICK_START.md)** — Installation, first module (2 min), authentication

### Quick Installation

```bash
cargo install chopin-cli
chopin new my-api && cd my-api
REUSEPORT=true cargo run --release --features perf
```

Your API is now serving 650K+ req/s 🚀

---

## 📚 Documentation

Complete guides for every use case:

| Document | Details |
|----------|---------|
| **[QUICK_START.md](docs/QUICK_START.md)** | Installation, first app, auth, database (5 min) |
| **[FEATURES.md](docs/FEATURES.md)** | Complete feature matrix, security options, RBAC |
| **[BENCHMARKS.md](docs/BENCHMARKS.md)** | Performance comparisons, cost analysis, optimization |
| **[modular-architecture.md](docs/modular-architecture.md)** | MVSR pattern, ChopinModule trait, hub-and-spoke design |
| **[json-performance.md](docs/json-performance.md)** | SIMD JSON, allocator tuning, zero-alloc hot paths |
| **[debugging-and-logging.md](docs/debugging-and-logging.md)** | Request logging, error traces, debugging guide |
| **[ARCHITECTURE.md](ARCHITECTURE.md)** | System design, component architecture, design principles |
| **[Website & Tutorials](https://kowito.github.io/chopin/)** | Interactive tutorials, full documentation |

---

## 💡 Key Concepts

### Type-Safe Modularity

Every feature is a `ChopinModule` with compile-time route verification:

```rust
impl ChopinModule for BlogModule {
    fn name(&self) -> &str { "blog" }
    fn routes(&self) -> Router<AppState> {
        Router::new()
            .route("/posts", get(list_posts).post(create_post))
            .route("/posts/:id", get(get_post))
    }
}
```

### Authentication & RBAC (Macros)

```rust
#[login_required]
async fn get_profile() -> Result<Json<ApiResponse<UserProfile>>, ChopinError> {
    Ok(Json(ApiResponse::success(profile)))
}

#[permission_required("can_publish_post")]
async fn publish_post() -> Result<Json<ApiResponse<Post>>, ChopinError> {
    // Only users with "can_publish_post" permission pass
    Ok(Json(ApiResponse::success(post)))
}
```

### Hub-and-Spoke Architecture

```
Your App
  ├─ BlogModule
  ├─ AuthModule
  └─ BillingModule
       ↓
    chopin-core (thin hub)
```

No modules depend on each other → zero circular dependencies → perfect monorepos.

## 🎯 Examples

Real-world project templates:

| Example | Description |
|---------|-------------|
| **[hello-world](chopin-examples/hello-world/)** | Minimal Chopin API (3 lines) |
| **[basic-api](chopin-examples/basic-api/)** | CRUD with auth and database |
| **[performance-mode](chopin-examples/performance-mode/)** | Maximum throughput configuration |
| **[benchmark](chopin-examples/benchmark/)** | TechEmpower-style benchmarks |

---

## Your First Modular App (2 minutes)

**See [QUICK_START.md](docs/QUICK_START.md) for complete code examples:**
- Step-by-step module creation
- Authentication & RBAC macros
- Built-in auth endpoints (signup, login, logout)
- Database integration
- Permission checking and fine-grained access control

---

## 🔥 The Secret Sauce

Chopin achieves extreme performance through:

1. **Unified ChopinService** — Raw hyper HTTP/1.1 with FastRoute zero-alloc fast path
2. **sonic-rs SIMD** — 40% faster JSON serialization (AVX2/NEON)
3. **mimalloc** — Microsoft's high-concurrency allocator
4. **Zero-alloc bodies** — `ChopinBody` avoids Box::pin overhead
5. **Cached headers** — Lock-free Date header updated every 500ms
6. **CPU-specific builds** — Native SIMD for your hardware

**Enable with:**
```bash
REUSEPORT=true cargo run --release --features perf
```

This gives you SO_REUSEPORT per-core workers, TCP_NODELAY, FastRoute, mimalloc, and sonic-rs.

---

## 💡 Migration from Axum

Chopin is built on Axum — **7% faster with zero breaking changes:**

```rust
// Before (Axum)
let app = Router::new()
    .route("/users", get(list_users));
axum::serve(listener, app).await?;

// After (Chopin) — 7% faster + auth + DB + OpenAPI
let app = App::new().await?
    .route("/users", get(list_users));
app.run().await?;
```

**What you get:**
- ✅ All Axum extractors/middleware work unchanged
- ✅ Full Tower/hyper compatibility
- ✅ 7% higher throughput + 12% lower latency
- ✅ Built-in auth, database, OpenAPI, caching, file uploads

---

## 📊 Use Cases

- **Fintech APIs** — Low latency (612µs), high throughput (657K req/s)
- **Gaming backends** — 3.75ms p99, predictable performance under load
- **Microservices** — Lightweight, high-scale internal APIs
- **SaaS platforms** — Production features out of the box, 50%+ cost savings

---

## 🤝 Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

**Areas we'd love help with:**
- More database adapters (MongoDB, DynamoDB)
- WebSocket examples
- gRPC integration
- Benchmark improvements
- Documentation

---

## ⚖️ License

**WTFPL** (Do What The Fuck You Want To Public License)

See [LICENSE](LICENSE) for details.

---

## 🌟 Ready to Build?

```bash
cargo install chopin-cli
chopin new my-api
cd my-api
REUSEPORT=true cargo run --release --features perf
```

**[Quick Start →](docs/QUICK_START.md) • [Features →](docs/FEATURES.md) • [Benchmarks →](docs/BENCHMARKS.md) • [Website](https://kowito.github.io/chopin/)**

---

<p align="center">
  Made with 🎹 by the Chopin team<br>
  <em>High-fidelity engineering for the modern virtuoso</em>
</p>