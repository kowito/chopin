use crate::{Executor, Model, OrmError, OrmResult, PgValue};

/// A smart wrapper around a `Model` tracking modified columns dynamically.
///
/// Used to perform targeted, minimal `UPDATE` or `INSERT` queries natively tracking state changes.
pub struct ActiveModel<M: Model> {
    pub inner: M,
    pub changes: Vec<(&'static str, PgValue)>,
    pub is_new: bool,
}

impl<M: Model> ActiveModel<M> {
    /// Create a new `ActiveModel` for an entity that hasn't been saved yet.
    pub fn new_insert(model: M) -> Self {
        Self {
            inner: model,
            changes: Vec::new(),
            is_new: true,
        }
    }

    /// Wrap an existing model into an `ActiveModel` state tracker for updates.
    pub fn from_model(model: M) -> Self {
        Self {
            inner: model,
            changes: Vec::new(),
            is_new: false,
        }
    }

    /// Flag a column as modified and stage its value for an upcoming transaction.
    pub fn set<T: crate::ToSql>(&mut self, column: &'static str, value: T) {
        if let Some(existing) = self.changes.iter_mut().find(|(c, _)| *c == column) {
            existing.1 = value.to_sql();
        } else {
            self.changes.push((column, value.to_sql()));
        }
    }

    /// Evaluates whether the underlying model lacks a primary key.
    pub fn is_new(&self) -> bool {
        self.is_new
    }

    /// Intelligently issues an `INSERT` or an `UPDATE` depending on `is_new()` state.
    pub fn save(&mut self, executor: &mut impl Executor) -> OrmResult<()> {
        if self.is_new() {
            self.insert(executor)?;
            self.is_new = false;
            Ok(())
        } else {
            self.update(executor)
        }
    }

    /// Executes a minimal `INSERT` leveraging exclusively the tracked changes dynamically compiled.
    pub fn insert(&mut self, executor: &mut impl Executor) -> OrmResult<()> {
        if self.changes.is_empty() {
            return self.inner.insert(executor);
        }

        let mut cols = Vec::new();
        let mut vals = Vec::new();
        for (c, v) in &self.changes {
            cols.push(*c);
            vals.push(v.clone());
        }

        let bindings: Vec<String> = (1..=cols.len()).map(|i| format!("${}", i)).collect();
        let query = format!(
            "INSERT INTO {} ({}) VALUES ({}) RETURNING {}",
            M::table_name(),
            cols.join(", "),
            bindings.join(", "),
            M::columns().join(", ")
        );

        let params: Vec<&dyn chopin_pg::types::ToSql> = vals.iter().map(|v| v as _).collect();
        let rows = executor.query(&query, &params)?;

        if let Some(row) = rows.first() {
            self.inner = M::from_row(row)?;
            self.changes.clear();
            Ok(())
        } else {
            Err(OrmError::ModelError(
                "Insert failed, no rows returned".to_string(),
            ))
        }
    }

    /// Executes a focused `UPDATE` leveraging exclusively the localized tracked columns dynamically compiled.
    pub fn update(&mut self, executor: &mut impl Executor) -> OrmResult<()> {
        if self.changes.is_empty() {
            return Ok(());
        }

        let mut set_clauses = Vec::new();
        let mut query_values = Vec::new();
        let mut param_idx = 1;

        for (col, val) in &self.changes {
            set_clauses.push(format!("{} = ${}", col, param_idx));
            query_values.push(val.clone());
            param_idx += 1;
        }

        let mut where_clauses = Vec::new();
        let pk_cols = M::primary_key_columns();
        let pk_vals = self.inner.primary_key_values();

        for (i, col) in pk_cols.iter().enumerate() {
            where_clauses.push(format!("{} = ${}", col, param_idx));
            query_values.push(pk_vals[i].clone());
            param_idx += 1;
        }

        let query = format!(
            "UPDATE {} SET {} WHERE {} RETURNING {}",
            M::table_name(),
            set_clauses.join(", "),
            where_clauses.join(" AND "),
            M::columns().join(", ")
        );

        let params: Vec<&dyn chopin_pg::types::ToSql> =
            query_values.iter().map(|v| v as _).collect();
        let rows = executor.query(&query, &params)?;

        if let Some(row) = rows.first() {
            self.inner = M::from_row(row)?;
            self.changes.clear();
            Ok(())
        } else {
            Err(OrmError::ModelError(
                "Update failed, no rows returned".to_string(),
            ))
        }
    }
}

impl<M: Model> From<M> for ActiveModel<M> {
    fn from(model: M) -> Self {
        ActiveModel::from_model(model)
    }
}
