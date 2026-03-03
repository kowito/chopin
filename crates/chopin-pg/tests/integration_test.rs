//! Integration tests for chopin-pg against a real PostgreSQL instance.
//!
//! **Prerequisites**: PostgreSQL running locally with:
//!   - user:     `chopin`
//!   - password: `chopin`
//!   - host:     `localhost:5432`
//!   - the user must have CREATEDB privilege
//!
//! If the connection cannot be established every test is **silently skipped**
//! (the test passes without doing anything). This makes CI without a DB still
//! green.
//!
//! Each test creates its own uniquely-named database (chopin_it_<pid>_<n>),
//! runs, then drops the database automatically via RAII. Tests are isolated
//! and can run in parallel without conflict.

use chopin_pg::{PgConfig, PgConnection, PgError, PgPool, PgPoolConfig, PgResult};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

// ─── TestDb — RAII isolated test database ─────────────────────────────────────

static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

const ADMIN_DB: &str = "postgres";

/// Admin connection config — connects to `postgres` DB so we can CREATE/DROP DBs.
fn admin_cfg() -> PgConfig {
    PgConfig::new("localhost", 5432, "chopin", "chopin", ADMIN_DB)
}

/// Per-test database guard.
/// On drop the database is automatically deleted.
struct TestDb {
    name: String,
    pub conn: PgConnection,
}

impl TestDb {
    /// Try to open a fresh isolated test database.
    /// Returns `None` if PostgreSQL is not reachable — the caller should
    /// `return` immediately to skip the test.
    fn open() -> Option<Self> {
        let mut admin = PgConnection::connect(&admin_cfg()).ok()?;

        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!("chopin_it_{}_{}", std::process::id(), n);

        // Drop any leftovers from a previous crashed run
        let _ = admin.execute_batch(&format!("DROP DATABASE IF EXISTS \"{}\"", name));
        admin
            .execute_batch(&format!("CREATE DATABASE \"{}\"", name))
            .ok()?;

        let test_cfg = PgConfig::new("localhost", 5432, "chopin", "chopin", &name);
        let conn = PgConnection::connect(&test_cfg).ok()?;

        Some(TestDb { name, conn })
    }

    /// Short-hand: open DB then immediately run setup SQL.
    fn with_schema(ddl: &str) -> Option<Self> {
        let mut db = Self::open()?;
        db.conn.execute_batch(ddl).ok()?;
        Some(db)
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        // Re-connect to admin DB to drop the test DB.
        // Ignore errors — best-effort cleanup.
        if let Ok(mut admin) = PgConnection::connect(&admin_cfg()) {
            let _ = admin.execute_batch(&format!("DROP DATABASE IF EXISTS \"{}\"", self.name));
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Create the standard `items` table used by most tests.
const ITEMS_DDL: &str = "
    CREATE TABLE items (
        id    SERIAL PRIMARY KEY,
        name  TEXT    NOT NULL,
        score INT     NOT NULL DEFAULT 0,
        active BOOLEAN NOT NULL DEFAULT TRUE
    );
";

// ─────────────────────────────────────────────────────────────────────────────
//  Phase 8.4 — Basic queries
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_connect_and_ping() {
    let Some(mut db) = TestDb::open() else { return };
    let rows = db.conn.query("SELECT 1 AS n", &[]).unwrap();
    assert_eq!(rows.len(), 1);
    let n: i32 = rows[0].get_typed(0).unwrap();
    assert_eq!(n, 1);
}

#[test]
fn test_select_integer_types() {
    let Some(mut db) = TestDb::open() else { return };
    let rows = db
        .conn
        .query(
            "SELECT $1::int2, $2::int4, $3::int8",
            &[&42i16, &1000i32, &9_000_000_000i64],
        )
        .unwrap();
    let v0: i16 = rows[0].get_typed(0).unwrap();
    let v1: i32 = rows[0].get_typed(1).unwrap();
    let v2: i64 = rows[0].get_typed(2).unwrap();
    assert_eq!(v0, 42);
    assert_eq!(v1, 1_000);
    assert_eq!(v2, 9_000_000_000);
}

#[test]
fn test_select_float_types() {
    let Some(mut db) = TestDb::open() else { return };
    let rows = db
        .conn
        .query("SELECT $1::float4, $2::float8", &[&1.5f32, &2.5f64])
        .unwrap();
    let f4: f32 = rows[0].get_typed(0).unwrap();
    let f8: f64 = rows[0].get_typed(1).unwrap();
    assert!((f4 - 1.5).abs() < 1e-6);
    assert!((f8 - 2.5).abs() < 1e-12);
}

#[test]
fn test_select_text_and_bool() {
    let Some(mut db) = TestDb::open() else { return };
    let rows = db
        .conn
        .query(
            "SELECT $1::text, $2::boolean, $3::boolean",
            &[&"hello", &true, &false],
        )
        .unwrap();
    let s: String = rows[0].get_typed(0).unwrap();
    let b1: bool = rows[0].get_typed(1).unwrap();
    let b2: bool = rows[0].get_typed(2).unwrap();
    assert_eq!(s, "hello");
    assert!(b1);
    assert!(!b2);
}

#[test]
fn test_insert_and_select_round_trip() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    db.conn
        .execute(
            "INSERT INTO items (name, score) VALUES ($1, $2)",
            &[&"alpha", &99i32],
        )
        .unwrap();

    let rows = db
        .conn
        .query("SELECT name, score FROM items WHERE name = $1", &[&"alpha"])
        .unwrap();

    assert_eq!(rows.len(), 1);
    let name: String = rows[0].get_typed(0).unwrap();
    let score: i32 = rows[0].get_typed(1).unwrap();
    assert_eq!(name, "alpha");
    assert_eq!(score, 99);
}

#[test]
fn test_affected_rows_insert_update_delete() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    let inserted = db
        .conn
        .execute(
            "INSERT INTO items (name, score) VALUES ('a', 1), ('b', 2), ('c', 3)",
            &[],
        )
        .unwrap();
    assert_eq!(inserted, 3);

    let updated = db
        .conn
        .execute("UPDATE items SET score = score + 10 WHERE score < 3", &[])
        .unwrap();
    assert_eq!(updated, 2);

    let deleted = db
        .conn
        .execute("DELETE FROM items WHERE name = 'c'", &[])
        .unwrap();
    assert_eq!(deleted, 1);
}

#[test]
fn test_query_one_returns_row() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    db.conn
        .execute(
            "INSERT INTO items (name, score) VALUES ($1, $2)",
            &[&"solo", &7i32],
        )
        .unwrap();

    let row = db
        .conn
        .query_one("SELECT name FROM items WHERE name = $1", &[&"solo"])
        .unwrap();

    let name: String = row.get_typed(0).unwrap();
    assert_eq!(name, "solo");
}

#[test]
fn test_query_one_missing_row_returns_error() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };
    let result = db
        .conn
        .query_one("SELECT id FROM items WHERE id = 9999", &[]);
    assert!(result.is_err(), "query_one on empty result should Err");
}

#[test]
fn test_execute_batch_multiple_statements() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    db.conn
        .execute_batch(
            "INSERT INTO items (name, score) VALUES ('x', 1);
             INSERT INTO items (name, score) VALUES ('y', 2);
             INSERT INTO items (name, score) VALUES ('z', 3);",
        )
        .unwrap();

    let rows = db.conn.query("SELECT count(*) FROM items", &[]).unwrap();
    let count: i64 = rows[0].get_typed(0).unwrap();
    assert_eq!(count, 3);
}

#[test]
fn test_null_parameter_and_result() {
    let Some(mut db) = TestDb::open() else { return };
    let null: Option<i32> = None;
    let rows = db.conn.query("SELECT $1::int4 IS NULL", &[&null]).unwrap();
    let is_null: bool = rows[0].get_typed(0).unwrap();
    assert!(is_null);
}

#[test]
fn test_query_simple_returns_rows() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    db.conn
        .execute("INSERT INTO items (name) VALUES ('simple')", &[])
        .unwrap();

    let rows = db.conn.query_simple("SELECT name FROM items").unwrap();
    assert!(!rows.is_empty());
}

#[test]
fn test_multiple_result_rows_ordered() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    db.conn
        .execute_batch("INSERT INTO items (name, score) VALUES ('a', 10), ('b', 20), ('c', 15);")
        .unwrap();

    let rows = db
        .conn
        .query("SELECT name FROM items ORDER BY score ASC", &[])
        .unwrap();

    assert_eq!(rows.len(), 3);
    let n0: String = rows[0].get_typed(0).unwrap();
    let n2: String = rows[2].get_typed(0).unwrap();
    assert_eq!(n0, "a");
    assert_eq!(n2, "b");
}

// ─────────────────────────────────────────────────────────────────────────────
//  Phase 8.5 — Connection Pool
// ─────────────────────────────────────────────────────────────────────────────

/// Return a PgPool backed by the given TestDb's database name.
fn make_pool(db_name: &str, max_size: usize) -> PgPool {
    let cfg = PgConfig::new("localhost", 5432, "chopin", "chopin", db_name);
    PgPool::new(cfg, max_size)
}

#[test]
fn test_pool_basic_checkout_and_return() {
    let Some(db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };
    let mut pool = make_pool(&db.name, 3);

    {
        let mut guard = pool.get().unwrap();
        guard
            .execute("INSERT INTO items (name) VALUES ('pool_row')", &[])
            .unwrap();
    } // guard drops → connection returned to pool

    assert_eq!(
        pool.idle_connections(),
        1,
        "connection should be back in pool"
    );
}

#[test]
fn test_pool_stats_track_checkouts() {
    let Some(db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };
    let mut pool = make_pool(&db.name, 5);

    // SAFETY: single-threaded; guards dropped before pool.
    let _g1: chopin_pg::ConnectionGuard<'static> =
        unsafe { std::mem::transmute(pool.get().unwrap()) };
    let _g2: chopin_pg::ConnectionGuard<'static> =
        unsafe { std::mem::transmute(pool.get().unwrap()) };

    assert_eq!(pool.active_connections(), 2);
    drop(_g1);
    assert_eq!(pool.active_connections(), 1);
    drop(_g2);
    assert_eq!(pool.active_connections(), 0);
    assert_eq!(pool.idle_connections(), 2);
}

#[test]
fn test_pool_try_get_exhausted_returns_error() {
    let Some(db) = TestDb::open() else { return };
    let mut pool = make_pool(&db.name, 1);

    // Exhaust pool (unsafe transmute to allow calling try_get while guard alive)
    // SAFETY: single-threaded; `_guard1` dropped before `pool`.
    let _guard1: chopin_pg::ConnectionGuard<'static> =
        unsafe { std::mem::transmute(pool.get().unwrap()) };
    // Pool is exhausted
    let result = pool.try_get();
    drop(_guard1);
    assert!(
        result.is_err(),
        "try_get on exhausted pool should return an error"
    );
}

#[test]
fn test_pool_get_timeout_when_exhausted() {
    let Some(db) = TestDb::open() else { return };

    let cfg = PgConfig::new("localhost", 5432, "chopin", "chopin", &db.name);
    let pool_cfg = PgPoolConfig::new()
        .max_size(1)
        .checkout_timeout(Duration::from_millis(50));
    let mut pool = PgPool::connect_with_config(cfg, pool_cfg).unwrap();

    // SAFETY: single-threaded test; `pool` outlives `held`; raw-pointer return
    // in ConnectionGuard::drop remains valid because `held` drops before `pool`
    // (reverse declaration order).  We transmute only to silence the lifetime
    // annotation so we can call `pool.get()` a second time.
    let held: chopin_pg::ConnectionGuard<'static> =
        unsafe { std::mem::transmute(pool.get().unwrap()) };
    let start = std::time::Instant::now();
    let result = pool.get(); // should time out
    let elapsed = start.elapsed();
    drop(held); // return connection before pool is destroyed

    assert!(result.is_err(), "get() should time out");
    assert!(
        elapsed >= Duration::from_millis(40),
        "should have waited ~50 ms"
    );
}

#[test]
fn test_pool_reap_removes_idle_connections() {
    let Some(db) = TestDb::open() else { return };
    let cfg = PgConfig::new("localhost", 5432, "chopin", "chopin", &db.name);
    let pool_cfg = PgPoolConfig::new()
        .max_size(3)
        .min_size(0)
        .idle_timeout(Duration::from_millis(1)); // expire immediately
    let mut pool = PgPool::connect_with_config(cfg, pool_cfg).unwrap();

    {
        let _g = pool.get().unwrap(); // create 1 connection, return immediately
    }
    assert_eq!(pool.idle_connections(), 1);

    std::thread::sleep(Duration::from_millis(100)); // let idle_timeout pass
    pool.reap();
    assert_eq!(pool.idle_connections(), 0);
}

#[test]
fn test_pool_multiple_sequential_queries() {
    let Some(db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };
    let mut pool = make_pool(&db.name, 2);

    for i in 0..5 {
        let mut g = pool.get().unwrap();
        g.execute(
            "INSERT INTO items (name, score) VALUES ($1, $2)",
            &[&format!("row{}", i), &i],
        )
        .unwrap();
    }

    let mut g = pool.get().unwrap();
    let rows = g.query("SELECT count(*) FROM items", &[]).unwrap();
    let count: i64 = rows[0].get_typed(0).unwrap();
    assert_eq!(count, 5);
}

// ─────────────────────────────────────────────────────────────────────────────
//  Phase 8.6 — COPY protocol
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_copy_in_write_data_finish() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    // COPY IN via write_data (raw CSV bytes)
    let mut writer = db
        .conn
        .copy_in("COPY items (name, score) FROM STDIN WITH (FORMAT CSV)")
        .unwrap();

    writer.write_data(b"copyA,100\n").unwrap();
    writer.write_data(b"copyB,200\n").unwrap();
    let rows_copied = writer.finish().unwrap();
    assert_eq!(rows_copied, 2);

    let rows = db.conn.query("SELECT count(*) FROM items", &[]).unwrap();
    let count: i64 = rows[0].get_typed(0).unwrap();
    assert_eq!(count, 2);
}

#[test]
fn test_copy_in_write_row_helper() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    let mut writer = db
        .conn
        .copy_in("COPY items (name, score) FROM STDIN WITH (FORMAT TEXT, DELIMITER '\t')")
        .unwrap();

    writer.write_row(&["row1", "10"]).unwrap();
    writer.write_row(&["row2", "20"]).unwrap();
    writer.write_row(&["row3", "30"]).unwrap();
    let rows_copied = writer.finish().unwrap();
    assert_eq!(rows_copied, 3);

    let rows = db.conn.query("SELECT count(*) FROM items", &[]).unwrap();
    let total: i64 = rows[0].get_typed(0).unwrap();
    assert_eq!(total, 3);
}

#[test]
fn test_copy_out_read_all() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    db.conn
        .execute_batch("INSERT INTO items (name, score) VALUES ('out1', 1), ('out2', 2);")
        .unwrap();

    let mut reader = db
        .conn
        .copy_out("COPY (SELECT name, score FROM items ORDER BY score) TO STDOUT WITH (FORMAT CSV)")
        .unwrap();

    let data = reader.read_all().unwrap();
    let text = String::from_utf8(data).unwrap();
    assert!(text.contains("out1"), "CSV should contain out1: {}", text);
    assert!(text.contains("out2"), "CSV should contain out2: {}", text);
    assert!(text.lines().count() >= 2);
}

#[test]
fn test_copy_out_read_data_chunks() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    db.conn
        .execute("INSERT INTO items (name, score) VALUES ('chunk', 42)", &[])
        .unwrap();

    let mut reader = db
        .conn
        .copy_out("COPY items (name, score) TO STDOUT WITH (FORMAT TEXT)")
        .unwrap();

    let mut all = Vec::new();
    while let Some(chunk) = reader.read_data().unwrap() {
        all.extend_from_slice(&chunk);
    }
    let s = String::from_utf8(all).unwrap();
    assert!(s.contains("chunk"), "expected 'chunk' in output: {}", s);
}

#[test]
fn test_copy_in_fail_aborts_transaction() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    let mut writer = db
        .conn
        .copy_in("COPY items (name, score) FROM STDIN WITH (FORMAT CSV)")
        .unwrap();

    writer.write_data(b"partial,1\n").ok();
    let _ = writer.fail("test abort"); // CopyFail — server should abort

    // Connection should be usable again after recovering the error
    let _ = db.conn.query_simple("ROLLBACK");
    let rows = db.conn.query("SELECT count(*) FROM items", &[]).unwrap();
    let count: i64 = rows[0].get_typed(0).unwrap();
    // Row should NOT be committed (copy was aborted)
    assert_eq!(count, 0, "aborted COPY should insert 0 rows");
}

// ─────────────────────────────────────────────────────────────────────────────
//  Phase 8.7 — LISTEN / NOTIFY
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_listen_then_notify_same_connection() {
    let Some(mut db) = TestDb::open() else { return };

    db.conn.listen("test_channel").unwrap();
    db.conn.notify("test_channel", "hello").unwrap();

    // After notify, poll the socket to receive the notification
    // (notify on the same connection triggers immediate receipt during next query)
    let _ = db.conn.query_simple("SELECT 1");
    let notifications = db.conn.drain_notifications();
    assert!(
        notifications.iter().any(|n| n.channel == "test_channel"),
        "should receive notification on test_channel"
    );
}

#[test]
fn test_notify_carries_payload() {
    let Some(mut db) = TestDb::open() else { return };

    db.conn.listen("payload_channel").unwrap();
    db.conn.notify("payload_channel", "my_payload").unwrap();

    let _ = db.conn.query_simple("SELECT 1");
    let notifications = db.conn.drain_notifications();
    assert!(
        notifications
            .iter()
            .any(|n| n.channel == "payload_channel" && n.payload == "my_payload"),
        "notification should carry the payload"
    );
}

#[test]
fn test_listen_two_channels() {
    let Some(mut db) = TestDb::open() else { return };

    db.conn.listen("chan_a").unwrap();
    db.conn.listen("chan_b").unwrap();
    db.conn.notify("chan_a", "msg_a").unwrap();
    db.conn.notify("chan_b", "msg_b").unwrap();

    let _ = db.conn.query_simple("SELECT 1");
    let notifications = db.conn.drain_notifications();

    assert!(
        notifications.iter().any(|n| n.channel == "chan_a"),
        "chan_a notification missing"
    );
    assert!(
        notifications.iter().any(|n| n.channel == "chan_b"),
        "chan_b notification missing"
    );
}

#[test]
fn test_unlisten_stops_notifications() {
    let Some(mut db) = TestDb::open() else { return };

    db.conn.listen("unsub_channel").unwrap();
    db.conn.unlisten("unsub_channel").unwrap();
    db.conn.notify("unsub_channel", "ignored").unwrap();

    let _ = db.conn.query_simple("SELECT 1");
    let notifications = db.conn.drain_notifications();
    let received = notifications.iter().any(|n| n.channel == "unsub_channel");
    assert!(!received, "should not receive notifications after unlisten");
}

#[test]
fn test_has_notifications_and_count() {
    let Some(mut db) = TestDb::open() else { return };

    assert!(!db.conn.has_notifications());
    assert_eq!(db.conn.notification_count(), 0);

    db.conn.listen("count_channel").unwrap();
    db.conn.notify("count_channel", "1").unwrap();
    db.conn.notify("count_channel", "2").unwrap();

    let _ = db.conn.query_simple("SELECT 1");
    assert!(db.conn.has_notifications());
    assert_eq!(db.conn.notification_count(), 2);

    db.conn.drain_notifications();
    assert!(!db.conn.has_notifications());
}

// ─────────────────────────────────────────────────────────────────────────────
//  Phase 8.8 — Transactions
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_begin_commit_visible() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    db.conn.begin().unwrap();
    db.conn
        .execute(
            "INSERT INTO items (name, score) VALUES ('committed', 1)",
            &[],
        )
        .unwrap();
    db.conn.commit().unwrap();

    let rows = db.conn.query("SELECT count(*) FROM items", &[]).unwrap();
    let count: i64 = rows[0].get_typed(0).unwrap();
    assert_eq!(count, 1, "committed row should be visible");
}

#[test]
fn test_begin_rollback_not_visible() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    db.conn.begin().unwrap();
    db.conn
        .execute(
            "INSERT INTO items (name, score) VALUES ('rolled_back', 99)",
            &[],
        )
        .unwrap();
    db.conn.rollback().unwrap();

    let rows = db.conn.query("SELECT count(*) FROM items", &[]).unwrap();
    let count: i64 = rows[0].get_typed(0).unwrap();
    assert_eq!(count, 0, "rolled back row should not be visible");
}

#[test]
fn test_transaction_closure_commits_on_ok() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    db.conn
        .transaction(|tx| {
            tx.execute(
                "INSERT INTO items (name, score) VALUES ('closure_ok', 5)",
                &[],
            )?;
            Ok(())
        })
        .unwrap();

    let rows = db.conn.query("SELECT count(*) FROM items", &[]).unwrap();
    let count: i64 = rows[0].get_typed(0).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_transaction_closure_rolls_back_on_err() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    let result = db.conn.transaction(|tx| {
        tx.execute(
            "INSERT INTO items (name, score) VALUES ('will_rollback', 5)",
            &[],
        )?;
        // Force an error — this triggers automatic rollback
        tx.execute("SELECT 1/0", &[])?;
        Ok(())
    });
    assert!(result.is_err(), "division by zero should propagate as Err");

    let rows = db.conn.query("SELECT count(*) FROM items", &[]).unwrap();
    let count: i64 = rows[0].get_typed(0).unwrap();
    assert_eq!(count, 0, "row should be rolled back after error");
}

#[test]
fn test_savepoint_partial_rollback() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    db.conn.begin().unwrap();

    db.conn
        .execute(
            "INSERT INTO items (name, score) VALUES ('before_sp', 1)",
            &[],
        )
        .unwrap();

    db.conn.savepoint("sp1").unwrap();

    db.conn
        .execute(
            "INSERT INTO items (name, score) VALUES ('after_sp', 2)",
            &[],
        )
        .unwrap();

    // Roll back to savepoint — only 'after_sp' is lost
    db.conn.rollback_to("sp1").unwrap();

    db.conn.commit().unwrap();

    let rows = db
        .conn
        .query("SELECT name FROM items ORDER BY id", &[])
        .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "only the row before the savepoint should remain"
    );
    let name: String = rows[0].get_typed(0).unwrap();
    assert_eq!(name, "before_sp");
}

#[test]
fn test_nested_transaction_via_savepoint() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };

    let result: PgResult<()> = db.conn.transaction(|outer| {
        outer.execute("INSERT INTO items (name, score) VALUES ('outer', 1)", &[])?;

        // Nested transaction: creates auto-savepoint
        let inner_result: PgResult<()> = outer.transaction(|inner| {
            inner.execute("INSERT INTO items (name, score) VALUES ('inner', 2)", &[])?;
            // fail the inner transaction
            inner.execute("SELECT 1/0", &[])?;
            Ok(())
        });
        assert!(inner_result.is_err());

        // outer transaction continues — only 'outer' row committed
        Ok(())
    });
    assert!(result.is_ok());

    let rows = db
        .conn
        .query("SELECT name FROM items ORDER BY id", &[])
        .unwrap();
    assert_eq!(rows.len(), 1, "only outer row should survive");
    let name: String = rows[0].get_typed(0).unwrap();
    assert_eq!(name, "outer");
}

// ─────────────────────────────────────────────────────────────────────────────
//  Phase 8.9 — Error conditions
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_bad_sql_returns_error() {
    let Some(mut db) = TestDb::open() else { return };
    let result = db.conn.query("THIS IS NOT SQL;", &[]);
    assert!(result.is_err(), "invalid SQL should return Err");
}

#[test]
fn test_wrong_type_cast_returns_error() {
    let Some(mut db) = TestDb::open() else { return };
    let result = db.conn.query("SELECT 'not_a_number'::integer", &[]);
    assert!(result.is_err(), "invalid cast should return Err");
}

#[test]
fn test_unique_constraint_violation() {
    let Some(mut db) = TestDb::with_schema("CREATE TABLE uniqs (id INT PRIMARY KEY, val TEXT);")
    else {
        return;
    };

    db.conn
        .execute("INSERT INTO uniqs (id, val) VALUES (1, 'a')", &[])
        .unwrap();

    let result = db
        .conn
        .execute("INSERT INTO uniqs (id, val) VALUES (1, 'b')", &[]);
    assert!(result.is_err(), "duplicate key should return Err");
}

#[test]
fn test_not_null_constraint_violation() {
    let Some(mut db) = TestDb::with_schema(ITEMS_DDL) else {
        return;
    };
    let result = db
        .conn
        .execute("INSERT INTO items (name, score) VALUES (NULL, 1)", &[]);
    assert!(result.is_err(), "NOT NULL violation should return Err");
}

#[test]
fn test_foreign_key_violation() {
    let Some(mut db) = TestDb::with_schema(
        "CREATE TABLE parents (id INT PRIMARY KEY);
         CREATE TABLE children (id INT PRIMARY KEY, parent_id INT REFERENCES parents(id));",
    ) else {
        return;
    };

    let result = db
        .conn
        .execute("INSERT INTO children (id, parent_id) VALUES (1, 999)", &[]);
    assert!(result.is_err(), "FK violation should return Err");
}

#[test]
fn test_error_is_server_variant() {
    let Some(mut db) = TestDb::open() else { return };
    let err = db
        .conn
        .query("SELECT bad_column FROM nonexistent_table", &[])
        .unwrap_err();
    assert!(
        matches!(err, PgError::Server { .. }),
        "should return PgError::Server, got: {:?}",
        err
    );
}

#[test]
fn test_error_classification_permanent() {
    let Some(mut db) = TestDb::open() else { return };
    let err = db.conn.query("BROKEN SQL;", &[]).unwrap_err();
    // Syntax errors should be classified as Permanent (not Transient)
    assert_eq!(
        err.classify(),
        chopin_pg::ErrorClass::Permanent,
        "syntax error should be Permanent, got: {:?}",
        err.classify()
    );
}

#[test]
fn test_connection_is_still_usable_after_error() {
    let Some(mut db) = TestDb::open() else { return };

    // First query: errors
    let _ = db.conn.query("SELECT 1/0", &[]);

    // Reset transaction state
    let _ = db.conn.query_simple("ROLLBACK");

    // Second query: should succeed
    let rows = db.conn.query("SELECT 42", &[]).unwrap();
    let v: i32 = rows[0].get_typed(0).unwrap();
    assert_eq!(
        v, 42,
        "connection should be reusable after error + rollback"
    );
}

#[test]
fn test_is_alive_returns_true_for_healthy_conn() {
    let Some(mut db) = TestDb::open() else { return };
    assert!(db.conn.is_alive(), "fresh connection should be alive");
}

#[test]
fn test_transaction_status_after_begin() {
    let Some(mut db) = TestDb::open() else { return };
    use chopin_pg::protocol::TransactionStatus;

    assert_eq!(db.conn.transaction_status(), TransactionStatus::Idle);
    db.conn.begin().unwrap();
    assert_eq!(
        db.conn.transaction_status(),
        TransactionStatus::InTransaction
    );
    db.conn.rollback().unwrap();
    assert_eq!(db.conn.transaction_status(), TransactionStatus::Idle);
}
