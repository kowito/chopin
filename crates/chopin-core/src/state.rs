//! Per-thread application state storage.
//!
//! Provides typed, per-thread state that survives the lifetime of the worker thread.
//! The idiomatic pattern is to call [`set_state`] inside
//! [`Chopin::with_worker_init`](crate::server::Chopin::with_worker_init) and read
//! state from handlers via [`Context::state`](crate::http::Context::state).
//!
//! ## Example
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use chopin_core::{get, set_state, Context, Response, Chopin};
//!
//! struct DbPool { /* ... */ }
//!
//! #[get("/users")]
//! fn list_users(ctx: Context) -> Response {
//!     let pool = ctx.state::<Arc<DbPool>>().expect("pool not initialised");
//!     // pool is Arc<DbPool> — cheap clone, thread-safe
//!     Response::text("ok")
//! }
//!
//! fn main() {
//!     Chopin::new()
//!         .mount_all_routes()
//!         .with_worker_init(|| {
//!             set_state(Arc::new(DbPool::new()));
//!         })
//!         .serve("0.0.0.0:8080")
//!         .unwrap();
//! }
//! ```
//!
//! ## Design notes
//!
//! Chopin uses a shared-nothing, thread-per-core model. Each worker thread is fully
//! independent; there is no shared mutable state between workers. [`set_state`] stores
//! a value in a `thread_local!` [`RefCell`], so it is safe to call even for non-`Sync`
//! types. For shared resources (e.g. connection pools, config), wrap in `Arc<T>` which
//! is `Clone` and cheap to clone per request.
use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static STATE_MAP: RefCell<HashMap<TypeId, Box<dyn Any>>> = RefCell::new(HashMap::new());
}

/// Store a per-thread value of type `T`.
///
/// Call this inside [`Chopin::with_worker_init`] to initialise state once per worker
/// thread before the event loop starts. Calling again replaces the previous value.
///
/// ## Example
///
/// ```rust,ignore
/// Chopin::new()
///     .mount_all_routes()
///     .with_worker_init(|| {
///         chopin_core::set_state(Arc::new(MyPool::new("postgres://localhost/db", 10)));
///     })
///     .serve("0.0.0.0:8080")
///     .unwrap();
/// ```
pub fn set_state<T: 'static>(val: T) {
    STATE_MAP.with(|m| {
        m.borrow_mut().insert(TypeId::of::<T>(), Box::new(val));
    });
}

/// Retrieve a clone of the per-thread state of type `T`.
///
/// Returns `None` if no value of type `T` has been set on this thread.
///
/// For types that are expensive to clone, prefer [`with_state`] to borrow without cloning.
///
/// ## Example
///
/// ```rust,ignore
/// let pool: Arc<MyPool> = chopin_core::get_state::<Arc<MyPool>>()
///     .expect("pool not initialised — call set_state in with_worker_init");
/// ```
pub fn get_state<T: Clone + 'static>() -> Option<T> {
    STATE_MAP.with(|m| {
        m.borrow()
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
            .cloned()
    })
}

/// Borrow the per-thread state of type `T` and apply a closure to it.
///
/// Use this for non-`Clone` types or when you want to avoid an unnecessary clone.
///
/// ## Example
///
/// ```rust,ignore
/// let result = chopin_core::with_state::<MyPool, _, _>(|pool| {
///     pool.query("SELECT 1")
/// });
/// ```
pub fn with_state<T: 'static, F: FnOnce(&T) -> R, R>(f: F) -> Option<R> {
    STATE_MAP.with(|m| {
        m.borrow()
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
            .map(f)
    })
}
