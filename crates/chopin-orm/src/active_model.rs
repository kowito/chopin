use crate::{Executor, Model, OrmError, OrmResult, PgValue};

/// State wrapper for a model field, tracking whether it has been modified.
#[derive(Clone, Debug, PartialEq)]
pub enum ActiveValue<V> {
    /// The value has been explicitly set and should be persisted.
    Set(V),
    /// The value remains unchanged from its last known database state.
    Unchanged(V),
    /// The value has never been set and is missing for this operation.
    NotSet,
}

impl<V> ActiveValue<V> {
    /// Returns true if the value is `Set`.
    pub fn is_set(&self) -> bool {
        matches!(self, Self::Set(_))
    }

    /// Unwraps the inner value, if present.
    pub fn into_value(self) -> Option<V> {
        match self {
            Self::Set(v) | Self::Unchanged(v) => Some(v),
            Self::NotSet => None,
        }
    }
}

/// A smart wrapper around a `Model` tracking modified columns dynamically.
///
/// Used to perform targeted, minimal `UPDATE` or `INSERT` queries that only
/// persist the columns that have actually changed.
pub struct ActiveModel<M: Model> {
    /// The underlying model instance.
    pub inner: M,
    /// Columns and their corresponding `ActiveValue` states.
    changes: Vec<(&'static str, ActiveValue<PgValue>)>,
    /// Whether this represents a new (unsaved) record.
    is_new: bool,
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
        let sql_val = value.to_sql();
        if let Some(existing) = self.changes.iter_mut().find(|(c, _)| *c == column) {
            existing.1 = ActiveValue::Set(sql_val);
        } else {
            self.changes.push((column, ActiveValue::Set(sql_val)));
        }
    }

    /// Returns whether any columns have been modified.
    pub fn has_changes(&self) -> bool {
        self.changes.iter().any(|(_, v)| v.is_set())
    }

    /// Returns the list of changed column names.
    pub fn changed_columns(&self) -> Vec<&'static str> {
        self.changes
            .iter()
            .filter(|(_, v)| v.is_set())
            .map(|(c, _)| *c)
            .collect()
    }

    /// Evaluates whether the underlying model has not been persisted yet.
    pub fn is_new(&self) -> bool {
        self.is_new
    }

    /// Validates the model, returning an error if validation fails.
    fn validate(&self) -> OrmResult<()> {
        self.inner.validate_or_err()
    }

    /// Intelligently issues an `INSERT` or `UPDATE` depending on `is_new()` state.
    ///
    /// Validates the model before persisting.
    pub fn save(&mut self, executor: &mut impl Executor) -> OrmResult<()> {
        self.validate()?;
        if self.is_new() {
            self.insert(executor)?;
            self.is_new = false;
            Ok(())
        } else {
            self.update(executor)
        }
    }

    /// Executes a minimal `INSERT` using only tracked changes.
    ///
    /// Validates the model before persisting.
    pub fn insert(&mut self, executor: &mut impl Executor) -> OrmResult<()> {
        self.validate()?;

        // If no changes, fall back to inserting everything from the inner model
        if !self.has_changes() {
            return self.inner.insert(executor);
        }

        let mut cols = Vec::new();
        let mut vals = Vec::new();
        for (c, v) in &self.changes {
            if let ActiveValue::Set(val) = v {
                cols.push(*c);
                vals.push(val.clone());
            }
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

    /// Executes a focused `UPDATE` persisting only the changed columns.
    ///
    /// Validates the model before persisting. No-op if nothing has changed.
    pub fn update(&mut self, executor: &mut impl Executor) -> OrmResult<()> {
        self.validate()?;
        if !self.has_changes() {
            return Ok(());
        }

        let mut set_clauses = Vec::new();
        let mut query_values = Vec::new();
        let mut param_idx = 1;

        for (col, val) in &self.changes {
            if let ActiveValue::Set(v) = val {
                set_clauses.push(format!("{} = ${}", col, param_idx));
                query_values.push(v.clone());
                param_idx += 1;
            }
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
