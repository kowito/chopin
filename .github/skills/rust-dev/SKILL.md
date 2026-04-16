---
name: rust-dev
description: 'Read, write, and fix Rust code. Use for analyzing Rust errors, generating idiomatic Rust with ownership/borrowing/async patterns, fixing compilation errors, refactoring for performance and safety, and running cargo check/clippy/test.'
argument-hint: 'Describe the Rust task: fix errors, write a function, refactor a module, etc.'
---

# Rust Developer

Analyze, write, and fix Rust code iteratively until it compiles and passes tests.

## When to Use
- Fix or diagnose Rust compilation errors
- Write new Rust functions, structs, traits, or modules
- Refactor Rust code for performance, safety, or idiomatic style
- Add or fix async/await code, ownership patterns, or lifetime annotations
- Lint and clean up Rust code with Clippy

## Procedure

### 1. Understand the Task
- Read the relevant source files before making changes
- Identify the exact error, goal, or refactor target
- Note ownership, lifetime, and async constraints in context

### 2. Implement Changes
- Prefer idiomatic Rust: use `?` for error propagation, `impl Trait` for flexibility, iterators over manual loops
- Follow ownership rules: minimize clones, prefer borrows, use `Arc`/`Mutex` only when shared mutability is necessary
- For async code: use `.await` correctly, avoid blocking in async contexts, prefer `tokio` patterns already in the codebase
- Match the existing code style and module conventions

### 3. Verify with `cargo check`
Run `cargo check` to catch compile errors quickly:
```
cargo check
```
- If errors remain, read the compiler output carefully — Rust errors are precise
- Fix each error and re-run until clean
- For workspace crates, use `cargo check -p <crate-name>` to target a specific crate

### 4. Lint with `cargo clippy`
```
cargo clippy
```
- Address `warning` and `error` level lints
- Use `#[allow(...)]` sparingly and only with justification
- Clippy suggestions are usually idiomatic improvements — apply them

### 5. Validate with `cargo test`
```
cargo test
```
- Run the full test suite; for a single crate: `cargo test -p <crate-name>`
- If tests fail, diagnose whether the fix broke existing behavior or the test expectations need updating
- Do not delete or skip tests to make them pass

### 6. Iterate Until Green
Repeat steps 3–5 until:
- `cargo check` exits with no errors
- `cargo clippy` shows no new warnings
- `cargo test` passes all relevant tests

### 7. Explain Changes
After completing the fix, briefly summarize:
- What was wrong or missing
- What pattern or Rust feature was applied
- Any trade-offs made (e.g., cloning vs. restructuring lifetimes)

## Idiomatic Rust Patterns

| Goal | Preferred Pattern |
|------|------------------|
| Error handling | `Result<T, E>` + `?` operator |
| Optional values | `Option<T>` + `map`/`and_then` |
| Shared ownership | `Arc<T>` (thread-safe), `Rc<T>` (single-thread) |
| Interior mutability | `Mutex<T>`, `RwLock<T>`, `Cell<T>`, `RefCell<T>` |
| Async runtime | `tokio::spawn`, `.await`, `async fn` |
| Collections | Iterators + `collect()` over manual loops |
| Newtype pattern | Wrap primitives for type safety |
| Builder pattern | For structs with many optional fields |

## Common Compiler Error Fixes

- **E0382 use of moved value**: borrow instead of move, or clone if ownership must be transferred
- **E0502 conflicting borrows**: restructure to avoid overlapping mutable/immutable borrows
- **E0277 trait not implemented**: add a `where` bound or implement the trait
- **E0106 missing lifetime specifier**: introduce lifetime annotations or use owned types
- **E0716 temporary dropped while borrowed**: extend the lifetime of the temporary with a `let` binding

## Quality Checklist
- [ ] `cargo check` passes with no errors
- [ ] `cargo clippy` introduces no new warnings
- [ ] `cargo test` passes all tests
- [ ] No unnecessary `clone()`, `unwrap()`, or `expect()` in production paths
- [ ] Async code does not block the executor
- [ ] Public APIs have doc comments if the codebase uses them
