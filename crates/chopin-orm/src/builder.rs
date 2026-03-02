use crate::{Model, OrmError, OrmResult, PgValue};
use std::marker::PhantomData;

pub struct Expr<M> {
    pub clause: String,
    pub params: Vec<PgValue>,
    _marker: PhantomData<M>,
}

impl<M> Expr<M> {
    pub fn new(clause: impl Into<String>, params: Vec<PgValue>) -> Self {
        Self {
            clause: clause.into(),
            params,
            _marker: PhantomData,
        }
    }
}

pub trait ColumnTrait<M: Model> {
    fn column_name(&self) -> &'static str;

    fn eq(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} = {{}}", self.column_name()), vec![val.to_sql()])
    }
    fn neq(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} != {{}}", self.column_name()), vec![val.to_sql()])
    }
    fn gt(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} > {{}}", self.column_name()), vec![val.to_sql()])
    }
    fn gte(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} >= {{}}", self.column_name()), vec![val.to_sql()])
    }
    fn lt(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} < {{}}", self.column_name()), vec![val.to_sql()])
    }
    fn lte(self, val: impl crate::ToSql) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} <= {{}}", self.column_name()), vec![val.to_sql()])
    }
    fn is_null(self) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} IS NULL", self.column_name()), vec![])
    }
    fn is_not_null(self) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("{} IS NOT NULL", self.column_name()), vec![])
    }
    fn count(self) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("COUNT({})", self.column_name()), vec![])
    }
    fn sum(self) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("SUM({})", self.column_name()), vec![])
    }
    fn max(self) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("MAX({})", self.column_name()), vec![])
    }
    fn min(self) -> Expr<M>
    where
        Self: Sized,
    {
        Expr::new(format!("MIN({})", self.column_name()), vec![])
    }
}

pub struct QueryBuilder<M> {
    _marker: PhantomData<M>,
    select_override: Option<Vec<Expr<M>>>,
    joins: Vec<String>,
    filters: Vec<Expr<M>>,
    group_by: Option<String>,
    having: Vec<Expr<M>>,
    order_by: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

impl<M: Model + Send + Sync> Default for QueryBuilder<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Model + Send + Sync> QueryBuilder<M> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
            select_override: None,
            joins: Vec::new(),
            filters: Vec::new(),
            group_by: None,
            having: Vec::new(),
            order_by: None,
            limit: None,
            offset: None,
        }
    }

    /// Add a WHERE filter clause, e.g., filter(Expr::new("age >= {}", vec![18.to_param()]))
    /// Or using DSL: filter(UserColumn::Age.gte(18.to_param()))
    /// Backwards compatibility raw signature with $1 etc is also supported if string literal passes without `{}`
    pub fn filter<E>(mut self, expr: E) -> Self
    where
        E: IntoExpr<M>,
    {
        self.filters.push(expr.into_expr());
        self
    }

    pub fn select_only<E: IntoExpr<M>>(mut self, exprs: Vec<E>) -> Self {
        self.select_override = Some(exprs.into_iter().map(|e| e.into_expr()).collect());
        self
    }

    pub fn join(mut self, clause: &str) -> Self {
        self.joins.push(clause.into());
        self
    }

    pub fn group_by(mut self, clause: &str) -> Self {
        self.group_by = Some(clause.into());
        self
    }

    pub fn having<E: IntoExpr<M>>(mut self, expr: E) -> Self {
        self.having.push(expr.into_expr());
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

    fn build_query(&self) -> (String, Vec<&PgValue>) {
        let mut all_params = Vec::new();
        let mut param_idx = 1;

        let mut resolve_expr = |expr: &Expr<M>, params: &mut Vec<*const PgValue>| -> String {
            let mut resolved = String::with_capacity(expr.clause.len());
            let mut chars = expr.clause.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '{' && chars.peek() == Some(&'}') {
                    chars.next();
                    resolved.push('$');
                    resolved.push_str(&param_idx.to_string());
                    param_idx += 1;
                } else {
                    resolved.push(c);
                }
            }
            for p in &expr.params {
                params.push(p as *const _);
            }
            resolved
        };

        // We use unsafe for `*const PgValue` mapping to `&PgValue` because closure borrows `all_params` mutably if we pass it directly.
        // It's perfectly safe here since `self` owns the `PgValue`s and outlives `all_params`.
        let mut temp_params = Vec::new();

        let select_clause = if let Some(exprs) = &self.select_override {
            let mapped: Vec<_> = exprs.iter().map(|e| resolve_expr(e, &mut temp_params)).collect();
            mapped.join(", ")
        } else {
            M::columns().join(", ")
        };

        let mut query = format!("SELECT {} FROM {}", select_clause, M::table_name());

        if !self.joins.is_empty() {
            query.push(' ');
            query.push_str(&self.joins.join(" "));
        }

        if !self.filters.is_empty() {
            query.push_str(" WHERE ");
            let filter_strings: Vec<_> = self.filters.iter().map(|e| resolve_expr(e, &mut temp_params)).collect();
            query.push_str(&filter_strings.join(" AND "));
        }

        if let Some(gb) = &self.group_by {
            query.push_str(" GROUP BY ");
            query.push_str(gb);
        }

        if !self.having.is_empty() {
            query.push_str(" HAVING ");
            let having_strings: Vec<_> = self.having.iter().map(|e| resolve_expr(e, &mut temp_params)).collect();
            query.push_str(&having_strings.join(" AND "));
        }

        for p_ptr in temp_params {
            all_params.push(unsafe { &*p_ptr });
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

        (query, all_params)
    }

    pub fn into_raw(self, executor: &mut impl crate::Executor) -> OrmResult<Vec<crate::Row>> {
        let (query, all_params) = self.build_query();
        println!("Query: {}, Params len: {}", query, all_params.len());
        let params_ref: Vec<&dyn chopin_pg::types::ToSql> = all_params.iter().map(|p| *p as _).collect();
        executor.query(&query, &params_ref)
    }

    pub fn all(self, executor: &mut impl crate::Executor) -> OrmResult<Vec<M>> {
        let (query, all_params) = self.build_query();

        let params_ref: Vec<&dyn chopin_pg::types::ToSql> =
            all_params.iter().map(|p| *p as _).collect();

        let rows = executor.query(&query, &params_ref)?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            result.push(M::from_row(&row)?);
        }
        Ok(result)
    }

    pub fn one(mut self, executor: &mut impl crate::Executor) -> OrmResult<Option<M>> {
        self.limit = Some(1);
        let mut all = self.all(executor)?;
        Ok(all.pop())
    }

    pub fn count(mut self, executor: &mut impl crate::Executor) -> OrmResult<i64> {
        self.select_override = Some(vec![Expr::new("COUNT(*)", vec![])]);
        let (query, all_params) = self.build_query();

        let params_ref: Vec<&dyn chopin_pg::types::ToSql> =
            all_params.iter().map(|p| *p as _).collect();

        let rows = executor.query(&query, &params_ref)?;
        if let Some(row) = rows.first() {
            let val: PgValue = row.get(0).map_err(OrmError::from)?;
            return Ok(match val {
                PgValue::Int8(v) => v,
                PgValue::Int4(v) => v as i64,
                PgValue::Text(s) => s.parse().unwrap_or(0),
                _ => 0,
            });
        }
        Ok(0)
    }
}

pub trait IntoExpr<M> {
    fn into_expr(self) -> Expr<M>;
}

impl<M> IntoExpr<M> for Expr<M> {
    fn into_expr(self) -> Expr<M> {
        self
    }
}

// For backwards compatibility filter("age = $1", vec![...])
impl<M, S: Into<String>> IntoExpr<M> for (S, Vec<PgValue>) {
    fn into_expr(self) -> Expr<M> {
        Expr::new(self.0.into(), self.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FromRow, Model};
    use chopin_pg::Row;

    struct MockModel {
        pub id: i32,
    }

    impl crate::Validate for MockModel {}

    impl FromRow for MockModel {
        fn from_row(_row: &Row) -> OrmResult<Self> {
            Ok(Self { id: 0 })
        }
    }

    impl Model for MockModel {
        fn table_name() -> &'static str {
            "mocks"
        }
        fn primary_key_columns() -> &'static [&'static str] {
            &["id"]
        }
        fn generated_columns() -> &'static [&'static str] {
            &["id"]
        }
        fn columns() -> &'static [&'static str] {
            &["id", "name"]
        }
        fn primary_key_values(&self) -> Vec<PgValue> {
            vec![PgValue::Int4(self.id)]
        }
        fn set_generated_values(&mut self, mut vals: Vec<PgValue>) -> OrmResult<()> {
            if vals.is_empty() { return Ok(()); }
            if let PgValue::Int4(v) = vals.remove(0) {
                self.id = v;
            }
            Ok(())
        }
        fn get_values(&self) -> Vec<PgValue> {
            vec![]
        }
        fn create_table_stmt() -> String {
            "".into()
        }
        fn column_definitions() -> Vec<(&'static str, &'static str)> {
            vec![]
        }
    }

    enum MockColumn {
        Id,
        Name,
    }

    impl ColumnTrait<MockModel> for MockColumn {
        fn column_name(&self) -> &'static str {
            match self {
                Self::Id => "id",
                Self::Name => "name",
            }
        }
    }

    #[test]
    fn test_query_builder_sql_generation() {
        let qb: QueryBuilder<MockModel> = QueryBuilder::new();
        assert_eq!(qb.build_query().0, "SELECT id, name FROM mocks");

        let qb = QueryBuilder::<MockModel>::new()
            .filter(("name = $1", vec![]))
            .filter(("id > $2", vec![]));
        assert_eq!(
            qb.build_query().0,
            "SELECT id, name FROM mocks WHERE name = $1 AND id > $2"
        );

        let qb = QueryBuilder::<MockModel>::new()
            .order_by("name DESC")
            .limit(10)
            .offset(5);
        assert_eq!(
            qb.build_query().0,
            "SELECT id, name FROM mocks ORDER BY name DESC LIMIT 10 OFFSET 5"
        );
    }

    #[test]
    fn test_order_by_without_where() {
        let qb = QueryBuilder::<MockModel>::new().order_by("id ASC");
        assert_eq!(qb.build_query().0, "SELECT id, name FROM mocks ORDER BY id ASC");
    }

    #[test]
    fn test_limit_only() {
        let qb = QueryBuilder::<MockModel>::new().limit(20);
        assert_eq!(qb.build_query().0, "SELECT id, name FROM mocks LIMIT 20");
    }

    #[test]
    fn test_offset_only() {
        let qb = QueryBuilder::<MockModel>::new().offset(15);
        assert_eq!(qb.build_query().0, "SELECT id, name FROM mocks OFFSET 15");
    }

    #[test]
    fn test_limit_and_offset_without_where() {
        let qb = QueryBuilder::<MockModel>::new().limit(5).offset(10);
        assert_eq!(qb.build_query().0, "SELECT id, name FROM mocks LIMIT 5 OFFSET 10");
    }

    #[test]
    fn test_multiple_filters() {
        let qb = QueryBuilder::<MockModel>::new()
            .filter(("id > $1", vec![]))
            .filter(("name = $2", vec![]))
            .filter(("active = $3", vec![]));
        assert_eq!(
            qb.build_query().0,
            "SELECT id, name FROM mocks WHERE id > $1 AND name = $2 AND active = $3"
        );
    }

    #[test]
    fn test_full_query_with_all_clauses() {
        let qb = QueryBuilder::<MockModel>::new()
            .filter(("status = $1", vec![]))
            .order_by("created_at DESC")
            .limit(25)
            .offset(50);
        assert_eq!(
            qb.build_query().0,
            "SELECT id, name FROM mocks WHERE status = $1 ORDER BY created_at DESC LIMIT 25 OFFSET 50"
        );
    }

    #[test]
    fn test_default_equals_new() {
        let qb_default: QueryBuilder<MockModel> = Default::default();
        let qb_new: QueryBuilder<MockModel> = QueryBuilder::new();
        assert_eq!(qb_default.build_query().0, qb_new.build_query().0);
    }

    #[test]
    fn test_no_clauses_is_plain_select() {
        let qb: QueryBuilder<MockModel> = QueryBuilder::new();
        assert_eq!(qb.build_query().0, "SELECT id, name FROM mocks");
    }

    #[test]
    fn test_dsl_generation() {
        let qb = QueryBuilder::<MockModel>::new()
            .filter(MockColumn::Id.gt(10))
            .filter(MockColumn::Name.eq("test"));
        let (sql, _) = qb.build_query();
        assert_eq!(sql, "SELECT id, name FROM mocks WHERE id > $1 AND name = $2");
    }
}
