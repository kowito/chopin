//! # chopin-orm
//!
//! An easy-to-use Object-Relational Mapper (ORM) for `chopin2`, backed by the high-performance
//! `chopin-pg` synchronous PostgreSQL driver.

pub use chopin_orm_macro::Model;
pub use chopin_pg::{
    PgResult, Row, connection::PgConnection, error::PgError, pool::PgPool, types::PgValue,
    types::ToSql,
};

pub mod builder;
pub use builder::QueryBuilder;
pub mod error;
pub use error::{OrmError, OrmResult};
pub mod active_model;
pub use active_model::ActiveModel;
pub mod migrations;
pub use migrations::{Index, Migration, MigrationManager, MigrationStatus};
pub mod mock;
pub use mock::MockExecutor;

/// A trait for types that can execute SQL queries and return results.
///
/// Implemented by `PgPool`, `PgConnection`, and `Transaction`.
pub trait Executor {
    /// Executes a command (e.g., INSERT, UPDATE, DELETE) and returns the number of affected rows.
    fn execute(&mut self, query: &str, params: &[&dyn chopin_pg::types::ToSql]) -> OrmResult<u64>;

    /// Executes a query and returns the resulting rows.
    fn query(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToSql],
    ) -> OrmResult<Vec<Row>>;
}

impl Executor for PgPool {
    fn execute(&mut self, query: &str, params: &[&dyn chopin_pg::types::ToSql]) -> OrmResult<u64> {
        self.get()
            .map_err(OrmError::from)?
            .execute(query, params)
            .map_err(OrmError::from)
    }

    fn query(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToSql],
    ) -> OrmResult<Vec<Row>> {
        self.get()
            .map_err(OrmError::from)?
            .query(query, params)
            .map_err(OrmError::from)
    }
}

impl Executor for PgConnection {
    fn execute(&mut self, query: &str, params: &[&dyn chopin_pg::types::ToSql]) -> OrmResult<u64> {
        chopin_pg::connection::PgConnection::execute(self, query, params).map_err(OrmError::from)
    }

    fn query(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToSql],
    ) -> OrmResult<Vec<Row>> {
        chopin_pg::connection::PgConnection::query(self, query, params).map_err(OrmError::from)
    }
}

/// A database transaction wrapper that automatically rolls back on drop
/// unless explicitly committed.
///
/// This ensures that if a panic or early return occurs, the transaction
/// is safely rolled back rather than leaving the connection in a dirty state.
pub struct Transaction<'a> {
    conn: &'a mut PgConnection,
    committed: bool,
}

impl<'a> Transaction<'a> {
    /// Begins a new transaction on the given connection.
    pub fn begin(conn: &'a mut PgConnection) -> OrmResult<Self> {
        conn.execute("BEGIN", &[]).map_err(OrmError::from)?;
        Ok(Self {
            conn,
            committed: false,
        })
    }

    /// Commits the transaction. Must be called explicitly to persist changes.
    pub fn commit(mut self) -> OrmResult<()> {
        self.committed = true;
        self.conn.execute("COMMIT", &[]).map_err(OrmError::from)?;
        Ok(())
    }

    /// Explicitly rolls back the transaction.
    pub fn rollback(mut self) -> OrmResult<()> {
        self.committed = true; // prevent double-rollback in Drop
        self.conn.execute("ROLLBACK", &[]).map_err(OrmError::from)?;
        Ok(())
    }
}

impl<'a> Drop for Transaction<'a> {
    fn drop(&mut self) {
        if !self.committed {
            // Best-effort rollback on drop — ignore errors since we may be
            // in a panic unwind where the connection is already broken.
            let _ = self.conn.execute("ROLLBACK", &[]);
            #[cfg(feature = "log")]
            log::warn!("Transaction dropped without explicit commit — rolled back");
        }
    }
}

impl<'a> Executor for Transaction<'a> {
    fn execute(&mut self, query: &str, params: &[&dyn chopin_pg::types::ToSql]) -> OrmResult<u64> {
        self.conn.execute(query, params).map_err(OrmError::from)
    }

    fn query(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToSql],
    ) -> OrmResult<Vec<Row>> {
        self.conn.query(query, params).map_err(OrmError::from)
    }
}

pub trait Validate {
    fn validate(&self) -> Result<(), Vec<String>> {
        Ok(()) // Default passes
    }
}

pub trait Model: FromRow + Validate + Sized + Send + Sync {
    fn table_name() -> &'static str;
    fn primary_key_columns() -> &'static [&'static str];
    fn generated_columns() -> &'static [&'static str];
    fn columns() -> &'static [&'static str];

    fn primary_key_values(&self) -> Vec<PgValue>;
    fn set_generated_values(&mut self, values: Vec<PgValue>) -> OrmResult<()>;
    fn get_values(&self) -> Vec<PgValue>;

    /// Generate the CREATE TABLE statement for this model
    fn create_table_stmt() -> String;

    /// Returns the literal raw SQL column definitions (name, type) for auto-migrations
    fn column_definitions() -> Vec<(&'static str, &'static str)>;

    /// Returns the list of indexes to natively enforce during migrations
    fn indexes() -> Vec<Index> {
        vec![]
    }

    /// Execute the CREATE TABLE statement against the database
    fn create_table(executor: &mut impl Executor) -> OrmResult<()> {
        executor.execute(&Self::create_table_stmt(), &[])?;
        Ok(())
    }

    /// Instantiate a `QueryBuilder` for this model dynamically.
    fn find() -> QueryBuilder<Self> {
        QueryBuilder::new()
    }

    /// Automatically diffs and migrates the table schema based on structural column metadata
    fn sync_schema(executor: &mut impl Executor) -> OrmResult<()> {
        Self::create_table(executor)?;

        // check existing columns
        let db_cols_query =
            "SELECT column_name FROM information_schema.columns WHERE table_name = $1";
        let table_name = Self::table_name();
        let params: Vec<&dyn chopin_pg::types::ToSql> = vec![&table_name];
        let rows = executor.query(db_cols_query, &params)?;

        let mut existing_cols = Vec::new();
        for row in rows {
            if let Ok(chopin_pg::PgValue::Text(val)) = row.get(0) {
                existing_cols.push(val.clone());
            }
        }

        let definitions = Self::column_definitions();
        for (col_name, col_def) in definitions {
            if !existing_cols.contains(&col_name.to_string()) {
                let alter_stmt = format!(
                    "ALTER TABLE {} ADD COLUMN {} {}",
                    Self::table_name(),
                    col_name,
                    col_def
                );
                executor.execute(&alter_stmt, &[])?;
                #[cfg(feature = "log")]
                log::info!(
                    "Auto-migrated {}: added column {}",
                    Self::table_name(),
                    col_name
                );
            }
        }

        for idx in Self::indexes() {
            let unique = if idx.unique { "UNIQUE " } else { "" };
            let create_idx = format!(
                "CREATE {}INDEX IF NOT EXISTS {} ON {} ({})",
                unique,
                idx.name,
                Self::table_name(),
                idx.columns.join(", ")
            );
            executor.execute(&create_idx, &[])?;
        }

        Ok(())
    }

    /// Insert the model into the database. Retrieves generated columns.
    fn insert(&mut self, executor: &mut impl Executor) -> OrmResult<()> {
        if let Err(errors) = self.validate() {
            return Err(OrmError::Validation(errors));
        }
        let all_cols = Self::columns();
        let gen_cols = Self::generated_columns();

        let mut cols = Vec::new();
        let values = self.get_values();
        let mut final_values = Vec::new();

        for (i, col) in all_cols.iter().enumerate() {
            if !gen_cols.contains(col) {
                cols.push(*col);
                final_values.push(values[i].clone());
            }
        }

        let bindings: Vec<String> = (1..=cols.len()).map(|i| format!("${}", i)).collect();
        let returning = if gen_cols.is_empty() {
            "".to_string()
        } else {
            format!(" RETURNING {}", gen_cols.join(", "))
        };

        let query = format!(
            "INSERT INTO {} ({}) VALUES ({}){}",
            Self::table_name(),
            cols.join(", "),
            bindings.join(", "),
            returning
        );

        let params: Vec<&dyn chopin_pg::types::ToSql> =
            final_values.iter().map(|v| v as _).collect();

        if gen_cols.is_empty() {
            executor.execute(&query, &params)?;
        } else {
            let rows = executor.query(&query, &params)?;
            if let Some(row) = rows.first() {
                let mut returned_vals = Vec::new();
                for i in 0..gen_cols.len() {
                    returned_vals.push(row.get(i)?);
                }
                self.set_generated_values(returned_vals)?;
            }
        }
        Ok(())
    }

    /// Insert the model or update it if the primary key conflicts
    fn upsert(&mut self, executor: &mut impl Executor) -> OrmResult<()> {
        if let Err(errors) = self.validate() {
            return Err(OrmError::Validation(errors));
        }
        let all_cols = Self::columns();
        let pk_cols = Self::primary_key_columns();
        let gen_cols = Self::generated_columns();

        if pk_cols.is_empty() {
            return Err(OrmError::ModelError(
                "Cannot upsert without primary keys".to_string(),
            ));
        }

        let mut cols = Vec::new();
        let values = self.get_values();
        let mut final_values = Vec::new();
        let mut set_clauses = Vec::new();

        for (i, col) in all_cols.iter().enumerate() {
            cols.push(*col);
            final_values.push(values[i].clone());
            if !pk_cols.contains(col) {
                set_clauses.push(format!("{0} = EXCLUDED.{0}", col));
            }
        }

        let bindings: Vec<String> = (1..=cols.len()).map(|i| format!("${}", i)).collect();

        // EXCLUDED is a postgres keyword referring to the row proposed for insertion
        let on_conflict = if set_clauses.is_empty() {
            "DO NOTHING".to_string()
        } else {
            format!("DO UPDATE SET {}", set_clauses.join(", "))
        };

        let returning = if gen_cols.is_empty() {
            "".to_string()
        } else {
            format!(" RETURNING {}", gen_cols.join(", "))
        };

        let query = format!(
            "INSERT INTO {0} ({1}) VALUES ({2}) ON CONFLICT ({3}) {4}{5}",
            Self::table_name(),
            cols.join(", "),
            bindings.join(", "),
            pk_cols.join(", "),
            on_conflict,
            returning
        );

        let params: Vec<&dyn chopin_pg::types::ToSql> =
            final_values.iter().map(|v| v as _).collect();

        if gen_cols.is_empty() {
            executor.execute(&query, &params)?;
        } else {
            let rows = executor.query(&query, &params)?;
            if let Some(row) = rows.first() {
                let mut returned_vals = Vec::new();
                for i in 0..gen_cols.len() {
                    returned_vals.push(row.get(i)?);
                }
                self.set_generated_values(returned_vals)?;
            }
        }
        Ok(())
    }

    /// Partially update the model, persisting only the specified columns to the database.
    fn update_columns(
        &self,
        executor: &mut impl Executor,
        update_columns: &[&str],
    ) -> OrmResult<Self> {
        if let Err(errors) = self.validate() {
            return Err(OrmError::Validation(errors));
        }
        let all_columns = Self::columns();
        let all_values = self.get_values();

        let mut set_clauses = Vec::new();
        let mut query_values = Vec::new();
        let mut param_idx = 1;

        for col in update_columns {
            if let Some(pos) = all_columns.iter().position(|c| c == col) {
                set_clauses.push(format!("{} = ${}", col, param_idx));
                query_values.push(all_values[pos].clone());
                param_idx += 1;
            } else {
                return Err(OrmError::ModelError(format!("Column not found: {}", col)));
            }
        }

        if set_clauses.is_empty() {
            return Err(OrmError::ModelError(
                "No valid columns provided for partial update".into(),
            ));
        }

        // Add primary key to WHERE clause
        let pk_cols = Self::primary_key_columns();
        let pk_vals = self.primary_key_values();

        let mut where_clauses = Vec::new();
        for (i, pk_col) in pk_cols.iter().enumerate() {
            where_clauses.push(format!("{} = ${}", pk_col, param_idx));
            query_values.push(pk_vals[i].clone());
            param_idx += 1;
        }

        let query = format!(
            "UPDATE {} SET {} WHERE {} RETURNING {}",
            Self::table_name(),
            set_clauses.join(", "),
            where_clauses.join(" AND "),
            Self::columns().join(", ")
        );

        let params_ref: Vec<&dyn chopin_pg::types::ToSql> =
            query_values.iter().map(|v| v as _).collect();
        let rows = executor.query(&query, &params_ref)?;

        if let Some(row) = rows.first() {
            Self::from_row(row)
        } else {
            Err(OrmError::ModelError(
                "Update failed, no rows returned".into(),
            ))
        }
    }

    /// Update the model in the database matching its primary key.
    fn update(&self, executor: &mut impl Executor) -> OrmResult<()> {
        if let Err(errors) = self.validate() {
            return Err(OrmError::Validation(errors));
        }
        let cols = Self::columns();
        let pk_cols = Self::primary_key_columns();

        if pk_cols.is_empty() {
            return Err(OrmError::ModelError(
                "Cannot update without primary keys".to_string(),
            ));
        }

        let mut set_clauses = Vec::new();
        let mut param_idx = 1;
        let values = self.get_values();
        let mut query_values = Vec::new();

        for (i, col) in cols.iter().enumerate() {
            if !pk_cols.contains(col) {
                set_clauses.push(format!("{} = ${}", col, param_idx));
                query_values.push(values[i].clone());
                param_idx += 1;
            }
        }

        if set_clauses.is_empty() {
            return Ok(()); // Nothing to update
        }

        let mut where_clauses = Vec::new();
        let pk_values = self.primary_key_values();
        for (i, pk_col) in pk_cols.iter().enumerate() {
            where_clauses.push(format!("{} = ${}", pk_col, param_idx));
            query_values.push(pk_values[i].clone());
            param_idx += 1;
        }

        let query = format!(
            "UPDATE {} SET {} WHERE {}",
            Self::table_name(),
            set_clauses.join(", "),
            where_clauses.join(" AND ")
        );

        let params: Vec<&dyn chopin_pg::types::ToSql> =
            query_values.iter().map(|v| v as _).collect();
        executor.execute(&query, &params)?;
        Ok(())
    }

    /// Delete the model from the database.
    fn delete(&self, executor: &mut impl Executor) -> OrmResult<()> {
        let pk_cols = Self::primary_key_columns();
        if pk_cols.is_empty() {
            return Err(OrmError::ModelError(
                "Cannot delete without primary keys".to_string(),
            ));
        }

        let mut where_clauses = Vec::new();
        for (idx, pk_col) in (1..).zip(pk_cols.iter()) {
            where_clauses.push(format!("{} = ${}", pk_col, idx));
        }

        let query = format!(
            "DELETE FROM {} WHERE {}",
            Self::table_name(),
            where_clauses.join(" AND ")
        );

        let pk_values = self.primary_key_values();
        let params: Vec<&dyn chopin_pg::types::ToSql> = pk_values.iter().map(|v| v as _).collect();

        executor.execute(&query, &params)?;
        Ok(())
    }
}

pub trait FromRow: Sized {
    fn from_row(row: &Row) -> OrmResult<Self>;
}

pub trait ExtractValue: Sized {
    fn extract(row: &Row, col: &str) -> OrmResult<Self>;
    fn from_pg_value(val: PgValue) -> OrmResult<Self>;
}

// Implement ExtractValue for common types
impl ExtractValue for String {
    fn extract(row: &Row, col: &str) -> OrmResult<Self> {
        let val = row.get_by_name(col).map_err(OrmError::from)?;
        Self::from_pg_value(val)
    }
    fn from_pg_value(val: PgValue) -> OrmResult<Self> {
        match val {
            PgValue::Text(s) => Ok(s),
            _ => Err(OrmError::Extraction("Expected Text".into())),
        }
    }
}

impl ExtractValue for i32 {
    fn extract(row: &Row, col: &str) -> OrmResult<Self> {
        let val = row.get_by_name(col).map_err(OrmError::from)?;
        Self::from_pg_value(val)
    }
    fn from_pg_value(val: PgValue) -> OrmResult<Self> {
        match val {
            PgValue::Int4(v) => Ok(v),
            PgValue::Int2(v) => Ok(v as i32),
            PgValue::Text(s) => s
                .parse()
                .map_err(|_| OrmError::Extraction("Not an i32".into())),
            _ => Err(OrmError::Extraction("Expected Int4".into())),
        }
    }
}

impl ExtractValue for i64 {
    fn extract(row: &Row, col: &str) -> OrmResult<Self> {
        let val = row.get_by_name(col).map_err(OrmError::from)?;
        Self::from_pg_value(val)
    }
    fn from_pg_value(val: PgValue) -> OrmResult<Self> {
        match val {
            PgValue::Int8(v) => Ok(v),
            PgValue::Int4(v) => Ok(v as i64),
            PgValue::Int2(v) => Ok(v as i64),
            PgValue::Text(s) => s
                .parse()
                .map_err(|_| OrmError::Extraction("Not an i64".into())),
            _ => Err(OrmError::Extraction("Expected Int8".into())),
        }
    }
}

impl ExtractValue for bool {
    fn extract(row: &Row, col: &str) -> OrmResult<Self> {
        let val = row.get_by_name(col).map_err(OrmError::from)?;
        Self::from_pg_value(val)
    }
    fn from_pg_value(val: PgValue) -> OrmResult<Self> {
        match val {
            PgValue::Bool(v) => Ok(v),
            PgValue::Text(s) => Ok(s == "t" || s == "true" || s == "1"),
            _ => Err(OrmError::Extraction("Expected Bool".into())),
        }
    }
}

impl ExtractValue for f64 {
    fn extract(row: &Row, col: &str) -> OrmResult<Self> {
        let val = row.get_by_name(col).map_err(OrmError::from)?;
        Self::from_pg_value(val)
    }
    fn from_pg_value(val: PgValue) -> OrmResult<Self> {
        match val {
            PgValue::Float8(v) => Ok(v),
            PgValue::Float4(v) => Ok(v as f64),
            PgValue::Text(s) => s
                .parse()
                .map_err(|_| OrmError::Extraction("Not an f64".into())),
            _ => Err(OrmError::Extraction("Expected Float8".into())),
        }
    }
}

// Option wrapper
impl<T: ExtractValue> ExtractValue for Option<T> {
    fn extract(row: &Row, col: &str) -> OrmResult<Self> {
        let val = row.get_by_name(col).map_err(OrmError::from)?;
        if let PgValue::Null = val {
            return Ok(None);
        }
        T::from_pg_value(val).map(Some)
    }
    fn from_pg_value(val: PgValue) -> OrmResult<Self> {
        if let PgValue::Null = val {
            return Ok(None);
        }
        T::from_pg_value(val).map(Some)
    }
}

// ─── f32 ExtractValue ─────────────────────────────────────────────────────────

impl ExtractValue for f32 {
    fn extract(row: &Row, col: &str) -> OrmResult<Self> {
        let val = row.get_by_name(col).map_err(OrmError::from)?;
        Self::from_pg_value(val)
    }
    fn from_pg_value(val: PgValue) -> OrmResult<Self> {
        match val {
            PgValue::Float4(v) => Ok(v),
            PgValue::Float8(v) => Ok(v as f32),
            PgValue::Int4(n) => Ok(n as f32),
            PgValue::Int8(n) => Ok(n as f32),
            PgValue::Int2(n) => Ok(n as f32),
            PgValue::Text(s) => s
                .parse()
                .map_err(|_| OrmError::Extraction(format!("Cannot parse '{}' as f32", s))),
            _ => Err(OrmError::Extraction("Expected Float4".into())),
        }
    }
}

// ─── chrono::NaiveDateTime ExtractValue ───────────────────────────────────────

#[cfg(feature = "chrono")]
const PG_EPOCH_OFFSET_SECS: i64 = 946_684_800;

#[cfg(feature = "chrono")]
impl ExtractValue for chrono::NaiveDateTime {
    fn extract(row: &Row, col: &str) -> OrmResult<Self> {
        let val = row
            .get_by_name(col)
            .map_err(|e| OrmError::Extraction(format!("column '{}': {}", col, e)))?;
        Self::from_pg_value(val)
    }
    fn from_pg_value(val: PgValue) -> OrmResult<Self> {
        match val {
            PgValue::Timestamp(micros) | PgValue::Timestamptz(micros) => {
                let unix_micros = micros + PG_EPOCH_OFFSET_SECS * 1_000_000;
                let secs = unix_micros.div_euclid(1_000_000);
                let nsecs = (unix_micros.rem_euclid(1_000_000) * 1_000) as u32;
                chrono::DateTime::from_timestamp(secs, nsecs)
                    .map(|dt| dt.naive_utc())
                    .ok_or_else(|| {
                        OrmError::Extraction(format!(
                            "Invalid timestamp microseconds: {}",
                            micros
                        ))
                    })
            }
            PgValue::Text(s) | PgValue::Json(s) => {
                chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S%.f")
                    .or_else(|_| {
                        chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%.f")
                    })
                    .or_else(|_| {
                        chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                    })
                    .map_err(|e| {
                        OrmError::Extraction(format!(
                            "Cannot parse '{}' as NaiveDateTime: {}",
                            s, e
                        ))
                    })
            }
            PgValue::Null => Err(OrmError::Extraction(
                "Cannot extract NaiveDateTime from NULL — use Option<NaiveDateTime>".to_string(),
            )),
            other => Err(OrmError::Extraction(format!(
                "Cannot convert {:?} to NaiveDateTime",
                other
            ))),
        }
    }
}

// ─── rust_decimal::Decimal ExtractValue ───────────────────────────────────────

#[cfg(feature = "decimal")]
impl ExtractValue for rust_decimal::Decimal {
    fn extract(row: &Row, col: &str) -> OrmResult<Self> {
        let val = row
            .get_by_name(col)
            .map_err(|e| OrmError::Extraction(format!("column '{}': {}", col, e)))?;
        Self::from_pg_value(val)
    }
    fn from_pg_value(val: PgValue) -> OrmResult<Self> {
        use std::str::FromStr;
        match val {
            PgValue::Numeric(s) | PgValue::Text(s) => {
                rust_decimal::Decimal::from_str(&s).map_err(|e| {
                    OrmError::Extraction(format!("Cannot parse '{}' as Decimal: {}", s, e))
                })
            }
            PgValue::Float8(v) => rust_decimal::Decimal::from_f64_retain(v)
                .ok_or_else(|| {
                    OrmError::Extraction(format!("Cannot convert f64 {} to Decimal", v))
                }),
            PgValue::Float4(v) => rust_decimal::Decimal::from_f64_retain(v as f64)
                .ok_or_else(|| {
                    OrmError::Extraction(format!("Cannot convert f32 {} to Decimal", v))
                }),
            PgValue::Int4(n) => Ok(rust_decimal::Decimal::from(n)),
            PgValue::Int8(n) => Ok(rust_decimal::Decimal::from(n)),
            PgValue::Int2(n) => Ok(rust_decimal::Decimal::from(n)),
            PgValue::Null => Err(OrmError::Extraction(
                "Cannot extract Decimal from NULL — use Option<Decimal>".to_string(),
            )),
            other => Err(OrmError::Extraction(format!(
                "Cannot convert {:?} to Decimal",
                other
            ))),
        }
    }
}

pub trait HasForeignKey<M: Model> {
    /// Returns the table name of the child and a list of (child_column, parent_column) mappings.
    fn foreign_key_info() -> (&'static str, Vec<(&'static str, &'static str)>);
}

/// A transparent middleware executor that intercepts queries and parameters.
///
/// Under the `log` feature flag, this emits `tracing` debug logs containing executed SQL,
/// elapsed execution time, and parameter payload metrics.
pub struct LoggedExecutor<'a, E: Executor> {
    pub inner: &'a mut E,
}

impl<'a, E: Executor> LoggedExecutor<'a, E> {
    /// Wraps an existing `Executor` (like `PgConnection` or `PgPool`) in logging telemetry.
    pub fn new(executor: &'a mut E) -> Self {
        Self { inner: executor }
    }
}

impl<'a, E: Executor> Executor for LoggedExecutor<'a, E> {
    fn execute(&mut self, query: &str, params: &[&dyn chopin_pg::types::ToSql]) -> OrmResult<u64> {
        let start = std::time::Instant::now();
        let res = self.inner.execute(query, params);
        let elapsed = start.elapsed();
        #[cfg(feature = "log")]
        log::debug!(
            "execute ({}ms): {} | params: {:?}",
            elapsed.as_millis(),
            query,
            params.len()
        );
        #[cfg(not(feature = "log"))]
        let _ = elapsed;
        res
    }

    fn query(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToSql],
    ) -> OrmResult<Vec<chopin_pg::Row>> {
        let start = std::time::Instant::now();
        let res = self.inner.query(query, params);
        let elapsed = start.elapsed();
        #[cfg(feature = "log")]
        log::debug!(
            "query ({}ms): {} | params: {:?}",
            elapsed.as_millis(),
            query,
            params.len()
        );
        #[cfg(not(feature = "log"))]
        let _ = elapsed;
        res
    }
}
