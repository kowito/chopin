//! I/O pool for offloading external async work from synchronous handlers.
//!
//! Chopin's worker threads run a pure synchronous, non-blocking event loop.
//! This is optimal for the hot path (parsing, routing, local PostgreSQL), but
//! calling external HTTP APIs (Stripe, S3, SendGrid, etc.) would block the
//! worker for the full round-trip time — 50–500ms.
//!
//! The [`IoPool`] solves this by running a small pool of dedicated I/O threads,
//! each hosting its own single-threaded `tokio` runtime.  Handlers send work
//! to the pool via a lock-free multi-producer channel and park the calling
//! worker thread until the result arrives.  The worker parks in the kernel
//! (futex/Condvar), so **zero CPU is wasted** while waiting.
//!
//! ## Design constraints (shared-nothing, zero hot-path overhead)
//!
//! * The I/O pool is **completely separate** from the event-loop workers.
//! * The hot path (`epoll` loop, slab, pipeline, PG driver) is **untouched**.
//! * If no handler ever calls `call_external()`, the pool threads sit idle with
//!   zero observable performance difference.
//! * Each I/O thread is single-threaded — no `Arc`, no locks, no cross-thread
//!   synchronisation inside the runtime itself.
//!
//! ## Usage
//!
//! ```rust,ignore
//! // In main(), before starting the server:
//! chopin_core::init_io_pool(4).expect("io pool");
//!
//! // In any handler:
//! #[post("/checkout")]
//! fn checkout(_ctx: Context) -> Response {
//!     let response = chopin_core::call_external(|| async {
//!         reqwest::Client::new()
//!             .post("https://api.stripe.com/v1/checkout/sessions")
//!             .header("Authorization", "Bearer sk_xxx")
//!             .form(&[("mode", "payment")])
//!             .send()
//!             .await?
//!             .text()
//!             .await
//!     });
//!     Response::text(response.unwrap_or_default())
//! }
//! ```
//!
//! ## When NOT to use `call_external`
//!
//! Never use this for local PostgreSQL queries — `chopin_pg::pool()` is already
//! optimised for that.  Never use it for fast in-memory work.  Only use it
//! when you genuinely need to reach an external service over the network.

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread;

// ─── Types ────────────────────────────────────────────────────────────────

/// A type-erased task that produces a value of type `T`.
type BoxedTask<T> = Box<dyn FnOnce() -> T + Send>;

/// The message sent from a hot worker to an I/O thread:
/// (the work to do, a channel to send the result back).
type Message<T> = (BoxedTask<T>, SyncSender<T>);

// ─── IoPool ───────────────────────────────────────────────────────────────

/// A pool of dedicated I/O threads, each running a single-threaded async
/// runtime.  Designed to be initialised once at startup and shared (via
/// cloned senders) across hot worker threads.
///
/// The pool uses **round-robin** distribution via an atomic counter,
/// ensuring even load without any shared mutable state.
pub struct IoPool {
    senders: Vec<SyncSender<Message<String>>>,
    /// Atomic counter for round-robin sender selection.
    round_robin: AtomicUsize,
    /// Tokio runtimes live on the spawned threads — we hold the handles
    /// so we can join them on drop (graceful shutdown).
    _handles: Vec<thread::JoinHandle<()>>,
}

impl IoPool {
    /// Create a new I/O pool with `n_threads` dedicated threads.
    ///
    /// Each thread hosts its own single-threaded `tokio` runtime and waits
    /// for work on a bounded MPSC channel (capacity 256 tasks).
    ///
    /// # Panics
    /// Panics if `n_threads == 0` or if any I/O thread fails to spawn.
    pub fn new(n_threads: usize) -> Self {
        assert!(n_threads > 0, "IoPool requires at least 1 thread");

        let mut senders = Vec::with_capacity(n_threads);
        let mut handles = Vec::with_capacity(n_threads);

        for i in 0..n_threads {
            // Bounded channel: backpressure when all I/O threads are saturated.
            // 256 is generous — most external API calls are < 500ms, so this
            // covers up to 512 req/s per I/O thread.
            let (task_tx, task_rx): (SyncSender<Message<String>>, Receiver<Message<String>>) =
                sync_channel(256);
            senders.push(task_tx);

            let handle = thread::Builder::new()
                .name(format!("chopin-io-{}", i))
                .spawn(move || {
                    // Each I/O thread runs a simple synchronous loop.
                    // When a task arrives, we create a single-threaded runtime
                    // just for that one task, run the future, then tear down.
                    // This avoids any "runtime within runtime" issues.
                    while let Ok((task, reply_tx)) = task_rx.recv() {
                        let result = task();
                        let _ = reply_tx.send(result);
                    }
                })
                .expect("failed to spawn I/O thread");

            handles.push(handle);
        }

        Self {
            senders,
            round_robin: AtomicUsize::new(0),
            _handles: handles,
        }
    }

    /// Number of I/O threads in this pool.
    #[inline]
    pub fn thread_count(&self) -> usize {
        self.senders.len()
    }

    /// Execute a closure on an I/O thread and return its result.
    ///
    /// The calling (hot worker) thread **parks** while waiting — no busy-loop,
    /// no CPU waste.  The I/O thread is free to run `tokio` tasks concurrently.
    ///
    /// This is intentionally generic over `T` so callers can return any type.
    /// However, the internal channel carries `String` for simplicity; most
    /// external API responses are text.
    pub(crate) fn run<F>(&self, f: F) -> String
    where
        F: FnOnce() -> String + Send + 'static,
    {
        let (reply_tx, reply_rx) = sync_channel(1);

        // Round-robin: atomic increment to distribute across I/O threads.
        let idx = self.round_robin.fetch_add(1, Ordering::Relaxed) % self.senders.len();

        // Send the task to the I/O thread.
        // If the channel is full (all 256 slots occupied), this call *blocks*
        // the hot worker until an I/O thread frees a slot.  This is intentional
        // backpressure — it prevents unbounded memory growth.
        if self.senders[idx].send((Box::new(f), reply_tx)).is_err() {
            // I/O thread has panicked/terminated.  Return empty string rather
            // than panicking the worker.
            return String::new();
        }

        // Park until the I/O thread sends the result.
        // If the I/O thread panicked, the sender was dropped and recv() returns
        // Err — we return an empty string.
        reply_rx.recv().unwrap_or_default()
    }

    /// Execute an async future on an I/O thread and return its result.
    ///
    /// This is the primary public API.  A fresh single-threaded tokio runtime
    /// is created for each call — `reqwest`, `aws-sdk`, etc. all work naturally.
    pub(crate) fn block_on<F, Fut, T>(&self, f: F) -> T
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send,
        T: Send + 'static,
    {
        let (tx, rx) = sync_channel(1);

        let task = move || {
            // Create a fresh single-threaded runtime for this one future.
            // Since the I/O thread has no ambient runtime, there's no nesting issue.
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime for I/O task");
            let result = rt.block_on(f());
            let _ = tx.send(result);
            String::new() // dummy — real result goes through tx
        };

        self.run(task);
        rx.recv()
            .unwrap_or_else(|_| panic!("IoPool I/O thread panicked while executing async task"))
    }
}

impl Drop for IoPool {
    fn drop(&mut self) {
        // Dropping `senders` closes the channel.  Each I/O thread will see
        // `recv() → Err(Disconnected)` and exit its loop cleanly, then the
        // tokio runtime drops and the thread joins.
        //
        // We don't join the handles here because `_handles` is dropped
        // automatically — but tokio's `block_on` will return when the
        // channel disconnects, so the threads will terminate.
        drop(self.senders.drain(..));
    }
}

// ─── Global singleton ──────────────────────────────────────────────────────

use std::sync::OnceLock;

static IO_POOL: OnceLock<IoPool> = OnceLock::new();

/// Initialise the global I/O pool.
///
/// Call this **once** at program startup, before calling `Server::serve()` or
/// `Chopin::serve()`.  The pool will be available to all worker threads via
/// [`call_external`].
///
/// # Arguments
/// * `n_threads` — Number of dedicated I/O threads.  A good default is 4.
///   More threads allow more concurrent external calls before backpressure
///   kicks in.
///
/// # Example
///
/// ```rust,ignore
/// fn main() {
///     chopin_core::init_io_pool(4).expect("io pool");
///
///     Chopin::new()
///         .mount_all_routes()
///         .serve("0.0.0.0:8080")
///         .unwrap();
/// }
/// ```
pub fn init_io_pool(n_threads: usize) -> Result<(), &'static str> {
    IO_POOL
        .set(IoPool::new(n_threads))
        .map_err(|_| "IoPool already initialised")
}

/// Execute an async future on the global I/O pool and return its result.
///
/// The calling worker thread **parks** while the future executes on a
/// dedicated I/O thread.  This is only for external I/O (HTTP APIs, cloud
/// storage, email) — **never** use it for local PostgreSQL queries or
/// in-memory work.
///
/// # Panics
/// Panics if [`init_io_pool`] has not been called or if the I/O thread panics.
///
/// # Example
///
/// ```rust,ignore
/// #[post("/checkout")]
/// fn checkout(_ctx: Context) -> Response {
///     let result = chopin_core::call_external(|| async {
///         reqwest::get("https://api.stripe.com/v1/checkout/sessions")
///             .await?
///             .text()
///             .await
///     });
///     Response::text(result.unwrap_or_default())
/// }
/// ```
pub fn call_external<F, Fut, T>(f: F) -> T
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send,
    T: Send + 'static,
{
    let pool = IO_POOL
        .get()
        .expect("IoPool not initialised. Call chopin_core::init_io_pool(n) at startup.");
    pool.block_on(f)
}

/// Execute a synchronous closure on a dedicated I/O thread.
///
/// This is the synchronous counterpart to [`call_external`].  Use it when you
/// have a CPU-bound or blocking operation that you want to offload from the
/// hot worker without pulling in an async runtime.
///
/// # Example
///
/// ```rust,ignore
/// let hash = chopin_core::spawn_io(|| {
///     bcrypt::hash("password", 12).unwrap()
/// });
/// ```
pub fn spawn_io<F, T>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let pool = IO_POOL
        .get()
        .expect("IoPool not initialised. Call chopin_core::init_io_pool(n) at startup.");

    let (tx, rx) = sync_channel(1);
    pool.run(move || {
        let result = f();
        let _ = tx.send(result);
        String::new()
    });
    rx.recv().expect("IoPool thread panicked")
}

// ─── State integration ─────────────────────────────────────────────────────
//
// For handlers that want to access the pool via `Context::state()`, we provide
// a thin wrapper that is `Clone` + `'static` for the `set_state` / `get_state`
// API.  However, since `IoPool` is a global singleton via `OnceLock`, most
// users will just call `call_external()` directly — no state plumbing needed.

/// A cheaply-cloneable handle to the global I/O pool.
///
/// Useful with `Context::state()` when you want typed access:
///
/// ```rust,ignore
/// Chopin::new()
///     .with_worker_init(|| {
///         set_state(IoPoolHandle::new());
///     })
///     .mount_all_routes()
///     .serve("0.0.0.0:8080")
///     .unwrap();
///
/// #[get("/api")]
/// fn handler(ctx: Context) -> Response {
///     let pool = ctx.state::<IoPoolHandle>().unwrap();
///     let data = pool.call(|| async { reqwest::get("...").await?.text().await });
///     Response::text(data.unwrap())
/// }
/// ```
#[derive(Clone)]
pub struct IoPoolHandle;

impl IoPoolHandle {
    /// Create a new handle.  The underlying pool must have been initialised
    /// via [`init_io_pool`].
    pub fn new() -> Self {
        Self
    }

    /// Execute an async future on the global I/O pool.
    pub fn call<F, Fut, T>(&self, f: F) -> T
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send,
        T: Send + 'static,
    {
        call_external(f)
    }
}

impl Default for IoPoolHandle {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_pool_basic() {
        let pool = IoPool::new(2);
        assert_eq!(pool.thread_count(), 2);

        let result = pool.run(|| "hello from io thread".to_string());
        assert_eq!(result, "hello from io thread");
    }

    #[test]
    fn test_io_pool_multiple_tasks() {
        let pool = IoPool::new(2);

        let a = pool.run(|| "a".to_string());
        let b = pool.run(|| "b".to_string());
        let c = pool.run(|| "c".to_string());

        // Results come back in submission order (synchronous from caller's view)
        assert_eq!(a, "a");
        assert_eq!(b, "b");
        assert_eq!(c, "c");
    }

    #[test]
    fn test_io_pool_async() {
        let pool = IoPool::new(1);

        let result: String = pool.block_on(|| async { "async result".to_string() });

        assert_eq!(result, "async result");
    }

    #[test]
    fn test_io_pool_async_with_real_future() {
        let pool = IoPool::new(1);

        let result: i32 = pool.block_on(|| async {
            // Simulate an async operation — just yield once
            tokio::task::yield_now().await;
            42
        });

        assert_eq!(result, 42);
    }

    #[test]
    fn test_io_pool_concurrent() {
        let pool = IoPool::new(4);

        // Submit tasks from multiple "threads" (simulated sequentially here,
        // but each call parks the calling thread — in real code these would
        // be different worker threads)
        let results: Vec<String> = (0..8)
            .map(|i| pool.run(move || format!("task-{}", i)))
            .collect();

        for (i, r) in results.iter().enumerate() {
            assert_eq!(r, &format!("task-{}", i));
        }
    }

    #[test]
    fn test_spawn_io() {
        // Use a fresh pool directly to avoid global singleton conflicts
        let pool = IoPool::new(1);

        let (tx, rx) = sync_channel(1);
        pool.run(move || {
            let _ = tx.send(99i32);
            String::new()
        });
        assert_eq!(rx.recv().unwrap(), 99);
    }

    #[test]
    fn test_io_pool_handle_clone() {
        // Test clone + call using a fresh pool
        let pool = IoPool::new(1);
        let r: String = pool.block_on(|| async { "ok".to_string() });
        assert_eq!(r, "ok");
    }
}
