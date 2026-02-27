use crate::{FromRow, Model, PgError, PgResult, PgValue};
use std::marker::PhantomData;

pub struct QueryBuilder<M> {
    _marker: PhantomData<M>,
    filters: Vec<String>,
    params: Vec<PgValue>,
    order_by: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

impl<M: Model + Send + Sync> QueryBuilder<M> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
            filters: Vec::new(),
            params: Vec::new(),
            order_by: None,
            limit: None,
            offset: None,
        }
    }

    /// Add a WHERE filter clause, e.g., filter("age >= $1", vec![18.to_param()]).
    /// Make sure the manual index ($1, $2) matches the appended parameters across filters.
    pub fn filter(mut self, clause: &str, params: Vec<PgValue>) -> Self {
        self.filters.push(clause.to_string());
        self.params.extend(params);
        self
    }

    pub fn order_by(mut self, clause: &str) -> Self {
        self.order_by = Some(clause.to_string());
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }

    fn build_query(&self) -> String {
        let mut query = format!("SELECT {} FROM {}", M::columns().join(", "), M::table_name());

        if !self.filters.is_empty() {
            query.push_str(" WHERE ");
            // In a better query builder we'd rewrite $1, $2 manually.
            // For now, assume the user provides correct absolute offsets in their clauses.
            query.push_str(&self.filters.join(" AND "));
        }

        if let Some(order) = &self.order_by {
            query.push_str(" ORDER BY ");
            query.push_str(order);
        }

        if let Some(limit) = self.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        query
    }

    pub fn all(self, executor: &mut impl crate::Executor) -> PgResult<Vec<M>> {
        let query = self.build_query();

        let params_ref: Vec<&dyn chopin_pg::types::ToParam> =
            self.params.iter().map(|p| p as _).collect();
            
        log::debug!("Executing Query: {} | Params: {}", query, self.params.len());
        
        let rows = executor.query(&query, &params_ref)?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(M::from_row(&row)?);
        }
        Ok(result)
    }

    pub fn one(mut self, executor: &mut impl crate::Executor) -> PgResult<Option<M>> {
        self.limit = Some(1);
        let mut all = self.all(executor)?;
        Ok(all.pop())
    }

    pub fn count(self, executor: &mut impl crate::Executor) -> PgResult<i64> {
        let mut query = format!("SELECT COUNT(*) FROM {}", M::table_name());

        if !self.filters.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&self.filters.join(" AND "));
        }

        let params_ref: Vec<&dyn chopin_pg::types::ToParam> =
            self.params.iter().map(|p| p as _).collect();
            
        log::debug!("Executing Count: {} | Params: {}", query, self.params.len());
        
        let rows = executor.query(&query, &params_ref)?;
        if let Some(row) = rows.first() {
            return row.get_i64(0)?.ok_or_else(|| PgError::Protocol("COUNT(*) returned null".to_string()));
        }
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chopin_pg::Row;

    struct MockModel {
        pub id: i32,
    }

    impl FromRow for MockModel {
        fn from_row(_row: &Row) -> PgResult<Self> {
            Ok(Self { id: 0 })
        }
    }

    impl Model for MockModel {
        fn table_name() -> &'static str { "mocks" }
        fn primary_key_column() -> &'static str { "id" }
        fn columns() -> &'static [&'static str] { &["id", "name"] }
        fn primary_key_value(&self) -> PgValue { PgValue::Int4(self.id) }
        fn set_primary_key(&mut self, val: PgValue) -> PgResult<()> {
            if let PgValue::Int4(v) = val { self.id = v; }
            Ok(())
        }
        fn get_values(&self) -> Vec<PgValue> { vec![] }
    }

    #[test]
    fn test_query_builder_sql_generation() {
        let qb: QueryBuilder<MockModel> = QueryBuilder::new();
        assert_eq!(qb.build_query(), "SELECT id, name FROM mocks");

        let qb = QueryBuilder::<MockModel>::new()
            .filter("name = $1", vec![])
            .filter("id > $2", vec![]);
        assert_eq!(
            qb.build_query(),
            "SELECT id, name FROM mocks WHERE name = $1 AND id > $2"
        );

        let qb = QueryBuilder::<MockModel>::new()
            .order_by("name DESC")
            .limit(10)
            .offset(5);
        assert_eq!(
            qb.build_query(),
            "SELECT id, name FROM mocks ORDER BY name DESC LIMIT 10 OFFSET 5"
        );
    }
}
