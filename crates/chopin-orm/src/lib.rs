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

pub trait Executor {
    fn execute(&mut self, query: &str, params: &[&dyn chopin_pg::types::ToSql])
    -> OrmResult<u64>;
    fn query(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToSql],
    ) -> OrmResult<Vec<Row>>;
}

impl Executor for PgPool {
    fn execute(
        &mut self,
        query: &str,
        params: &[&dyn chopin_pg::types::ToSql],
    ) -> OrmResult<u64> {
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
        params: &[&dyn chopin_pg::types::ToSql],
    ) -> OrmResult<u64> {
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

pub trait Model: FromRow + Sized + Send + Sync {
    fn table_name() -> &'static str;
    fn primary_key_columns() -> &'static [&'static str];
    fn generated_columns() -> &'static [&'static str];
    fn columns() -> &'static [&'static str];

    fn primary_key_values(&self) -> Vec<PgValue>;
    fn set_generated_values(&mut self, values: Vec<PgValue>) -> OrmResult<()>;
    fn get_values(&self) -> Vec<PgValue>;

    /// Insert the model into the database. Retrieves generated columns.
    fn insert(&mut self, executor: &mut impl Executor) -> OrmResult<()> {
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
        let all_cols = Self::columns();
        let pk_cols = Self::primary_key_columns();
        let gen_cols = Self::generated_columns();

        if pk_cols.is_empty() {
            return Err(OrmError::ModelError("Cannot upsert without primary keys".to_string()));
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

    /// Update the model in the database matching its primary key.
    fn update(&self, executor: &mut impl Executor) -> OrmResult<()> {
        let cols = Self::columns();
        let pk_cols = Self::primary_key_columns();

        if pk_cols.is_empty() {
            return Err(OrmError::ModelError("Cannot update without primary keys".to_string()));
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

        let params: Vec<&dyn chopin_pg::types::ToSql> = query_values.iter().map(|v| v as _).collect();
        executor.execute(&query, &params)?;
        Ok(())
    }

    /// Delete the model from the database.
    fn delete(&self, executor: &mut impl Executor) -> OrmResult<()> {
        let pk_cols = Self::primary_key_columns();
        if pk_cols.is_empty() {
            return Err(OrmError::ModelError("Cannot delete without primary keys".to_string()));
        }

        let mut where_clauses = Vec::new();
        let mut idx = 1;
        for pk_col in pk_cols {
            where_clauses.push(format!("{} = ${}", pk_col, idx));
            idx += 1;
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
