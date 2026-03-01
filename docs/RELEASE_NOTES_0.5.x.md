# Chopin Release Notes: v0.5.0 – v0.5.7 (Codename: Nocturne)

The 0.5.x series, codenamed **Nocturne**, marks the most significant evolution of Chopin to date. We've transitioned from a proof-of-concept networking engine to a production-hardened web framework with industry-leading performance and modern ergonomics.

## 🚀 Performance: The Optimization Milestone
The core engine has been completely rebuilt for linear scaling on multi-core systems.
- **SO_REUSEPORT Core**: Implementation of kernel-level load balancing for zero-contention socket accept.
- **Slab Allocation**: $O(1)$ connection management using a custom slab allocator.
- **Zero-Copy Serialization**: Integrated `kowito-json` Schema-JIT engine achieving >30 GiB/s serialization.
- **Validated Benchmarks**: Fresh comparisons show Chopin outperforming Actix Web and Axum by 10-15% and Hyper by ~40% in raw throughput.

## 🎼 Ergonomics: Attribute-Based Routing
We have moved away from manual router registration to a declarative, macro-based system.
- **Implicit Discovery**: Using `#[get]`, `#[post]`, etc., handlers are now automatically discovered and mounted at startup via `Chopin::mount_all_routes()`.
- **Zero Runtime Overhead**: Route collection happens at compile-time/startup using the `inventory` pattern, maintaining our commitment to maximum efficiency.

- **Chopin CLI**: A new unified CLI tool for project scaffolding, architectural linting (`chopin check`), and production deployment (`chopin deploy docker`).
- **Authorization & Security**: Initial release of `chopin-auth` featuring zero-allocation JWT validation and role-based middleware.
- **Premium Documentation**: A new landing page and documentation site featuring a high-fidelity design.
- **Production Hardening**: Exhaustive cleanup of clippy lints, formatting, and unit tests across the workspace.
- **CI/CD Automation**: Fully automated version bumping and publishing to crates.io.

## 💎 New Component: Chopin ORM
The release of `chopin-orm` brings type-safe database interactions to the ecosystem.
- **Zero-Overhead Driver**: Built on `chopin-pg`, our raw syscall-based PostgreSQL driver.
- **Type-Safe Query Builder**: Compile-time checked queries without the performance penalty of traditional ORMs.
- **Linear Scaling**: The ORM matches the raw driver performance across 1k, 100k, and 1M row benchmarks.

## 📝 Version Summary
- **v0.5.0**: Groundwork for `SO_REUSEPORT` and performance hardening.
- **v0.5.1 - v0.5.2**: Launch of `chopin-orm` and `chopin-pg`.
- **v0.5.3**: Refactoring to Attribute Macros and Implicit Routing.
- **v0.5.4**: CI/CD optimization and benchmark synchronization with Actix/Axum.
- **v0.5.5 - v0.5.6**: Documentation update and ORM performance tuning.
- **v0.5.7**: Major Naming Convention Overhaul (`Response::text`, `ctx.param`, `router.layer`).

---
"Simple as a melody, fast as a nocturne."
