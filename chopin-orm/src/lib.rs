//! # chopin-orm
//!
//! An easy-to-use Object-Relational Mapper (ORM) for `chopin2`, backed by the high-performance
//! `chopin-pg` synchronous PostgreSQL driver.

pub use chopin_orm_macro::Model;
pub use chopin_pg::{
    PgResult, Row, connection::PgConnection, error::PgError, pool::PgPool, types::PgValue,
};

pub mod builder;
pub use builder::QueryBuilder;
pub mod error;
pub use error::{OrmError, OrmResult};

pub trait Executor {
    fn execute(&mut self, query: &str, params: &[&dyn chopin_pg::types::ToParam])
    -> OrmResult<u64>;
    fn query(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToParam],
    ) -> OrmResult<Vec<Row>>;
}

impl Executor for PgPool {
    fn execute(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToParam],
    ) -> OrmResult<u64> {
        self.get()
            .map_err(OrmError::from)?
            .execute(query, params)
            .map_err(OrmError::from)
    }

    fn query(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToParam],
    ) -> OrmResult<Vec<Row>> {
        self.get()
            .map_err(OrmError::from)?
            .query(query, params)
            .map_err(OrmError::from)
    }
}

pub struct Transaction<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> Transaction<'a> {
    pub fn begin(conn: &'a mut PgConnection) -> OrmResult<Self> {
        conn.execute("BEGIN", &[]).map_err(OrmError::from)?;
        Ok(Self { conn })
    }

    pub fn commit(self) -> OrmResult<()> {
        self.conn.execute("COMMIT", &[]).map_err(OrmError::from)?;
        Ok(())
    }

    pub fn rollback(self) -> OrmResult<()> {
        self.conn.execute("ROLLBACK", &[]).map_err(OrmError::from)?;
        Ok(())
    }
}

impl<'a> Executor for Transaction<'a> {
    fn execute(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToParam],
    ) -> OrmResult<u64> {
        self.conn.execute(query, params).map_err(OrmError::from)
    }

    fn query(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToParam],
    ) -> OrmResult<Vec<Row>> {
        self.conn.query(query, params).map_err(OrmError::from)
    }
}

pub trait Model: FromRow + Sized + Send + Sync {
    fn table_name() -> &'static str;
    fn primary_key_column() -> &'static str;
    fn columns() -> &'static [&'static str];

    fn primary_key_value(&self) -> PgValue;
    fn set_primary_key(&mut self, value: PgValue) -> OrmResult<()>;
    fn get_values(&self) -> Vec<PgValue>;

    /// Insert the model into the database. Updates the primary key if it's auto-generated.
    fn insert(&mut self, executor: &mut impl Executor) -> OrmResult<()> {
        let all_cols = Self::columns();
        let pk_col = Self::primary_key_column();

        let mut cols = Vec::new();
        let values = self.get_values();
        let mut final_values = Vec::new();

        for (i, col) in all_cols.iter().enumerate() {
            if *col != pk_col {
                cols.push(*col);
                final_values.push(values[i].clone());
            }
        }

        let bindings: Vec<String> = (1..=cols.len()).map(|i| format!("${}", i)).collect();
        let query = format!(
            "INSERT INTO {} ({}) VALUES ({}) RETURNING {}",
            Self::table_name(),
            cols.join(", "),
            bindings.join(", "),
            Self::primary_key_column()
        );

        let params: Vec<&dyn chopin_pg::types::ToParam> =
            final_values.iter().map(|v| v as _).collect();

        log::debug!(
            "Executing Insert: {} | Params: {}",
            query,
            final_values.len()
        );

        // We know we inserted one row, get returning pk.
        let rows = executor.query(&query, &params)?;
        if let Some(row) = rows.first() {
            let pk_val = row.get(0)?;
            self.set_primary_key(pk_val)?;
        }
        Ok(())
    }

    /// Insert the model or update it if the primary key conflicts
    fn upsert(&mut self, executor: &mut impl Executor) -> OrmResult<()> {
        let all_cols = Self::columns();
        let pk_col = Self::primary_key_column();

        let mut cols = Vec::new();
        let values = self.get_values();
        let mut final_values = Vec::new();
        let mut set_clauses = Vec::new();

        // Push the PK first because we must provide it to conflict.
        let pk_idx = all_cols.iter().position(|c| *c == pk_col).ok_or_else(|| {
            OrmError::ModelError("Primary key column missing from Model::columns()".to_string())
        })?;
        cols.push(pk_col);
        final_values.push(values[pk_idx].clone());

        for (i, col) in all_cols.iter().enumerate() {
            if *col != pk_col {
                cols.push(*col);
                final_values.push(values[i].clone());
                // EXCLUDED is a postgres keyword referring to the row proposed for insertion
                set_clauses.push(format!("{0} = EXCLUDED.{0}", col));
            }
        }

        // Add PK separately for bindings if passing manual ID, else it uses serial default
        let bindings: Vec<String> = (1..=cols.len()).map(|i| format!("${}", i)).collect();
        let query = format!(
            "INSERT INTO {0} ({1}) VALUES ({2}) ON CONFLICT ({3}) DO UPDATE SET {4} RETURNING {3}",
            Self::table_name(),
            cols.join(", "),
            bindings.join(", "),
            Self::primary_key_column(),
            set_clauses.join(", ")
        );

        let params: Vec<&dyn chopin_pg::types::ToParam> =
            final_values.iter().map(|v| v as _).collect();

        log::debug!(
            "Executing Upsert: {} | Params: {}",
            query,
            final_values.len()
        );

        // We know we inserted/updated one row, get returning pk.
        let rows = executor.query(&query, &params)?;
        if let Some(row) = rows.first() {
            let pk_val = row.get(0)?;
            self.set_primary_key(pk_val)?;
        }
        Ok(())
    }

    /// Update the model in the database matching its primary key.
    fn update(&self, executor: &mut impl Executor) -> OrmResult<()> {
        let cols = Self::columns();
        let mut set_clauses = Vec::new();
        let mut param_idx = 1;

        for col in cols {
            if *col == Self::primary_key_column() {
                continue; // don't update PK
            }
            set_clauses.push(format!("{} = ${}", col, param_idx));
            param_idx += 1;
        }

        let query = format!(
            "UPDATE {} SET {} WHERE {} = ${}",
            Self::table_name(),
            set_clauses.join(", "),
            Self::primary_key_column(),
            param_idx
        );

        let mut values = self.get_values();
        // Remove the primary key from values to match the query bindings order
        let pk_idx = cols
            .iter()
            .position(|c| *c == Self::primary_key_column())
            .ok_or_else(|| {
                OrmError::ModelError("Primary key column missing from Model::columns()".to_string())
            })?;
        let pk_val = values.remove(pk_idx);
        values.push(pk_val); // Put PK at the end (for WHERE clause)

        let params: Vec<&dyn chopin_pg::types::ToParam> = values.iter().map(|v| v as _).collect();
        log::debug!("Executing Update: {} | Params: {}", query, values.len());
        executor.execute(&query, &params)?;
        Ok(())
    }

    /// Delete the model from the database.
    fn delete(&self, executor: &mut impl Executor) -> OrmResult<()> {
        let query = format!(
            "DELETE FROM {} WHERE {} = $1",
            Self::table_name(),
            Self::primary_key_column()
        );

        let pk = self.primary_key_value();

        log::debug!("Executing Delete: {} | Params: 1", query);

        executor.execute(&query, &[&pk])?;
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
